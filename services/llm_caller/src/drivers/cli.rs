//! The shared part of the CLI drivers (≈ an abstract base class): run the program in its own temp folder,
//! feed stdin, read stdout one JSON line at a time, and let the driver say what each line means.
//! The process is killed when the call is dropped (client timeout or server deadline).

use crate::drivers::ImageArg;
use sdk::{ProgressSender, RpcError};
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};

// ============================================================================
// What one stdout line means (each driver has its own `parse_line`)
// ============================================================================

#[derive(Debug, PartialEq, Eq)]
pub enum LineEvent {
    /// Nothing interesting (init, status, …).
    Ignore,
    /// The model is reasoning or using a tool (not part of the answer).
    Thinking,
    /// New answer text.
    Text(String),
    /// The last line: the whole answer, or the CLI's own error report.
    Done(Result<String, RpcError>),
}

/// One CLI run.
pub struct CliRun {
    pub program: String,
    pub args: Vec<String>,
    /// Text written to stdin, then stdin is closed. `None` = stdin stays empty.
    pub stdin: Option<String>,
}

// ============================================================================
// Work folder (RAII: deleted on Drop, like C# `using`)
// ============================================================================

/// The call's own empty folder, holding only the images as `image-N.<ext>`.
pub struct WorkDir {
    dir: tempfile::TempDir,
    pub image_names: Vec<String>,
}

impl WorkDir {
    pub fn create(images: &[ImageArg]) -> Result<Self, RpcError> {
        let dir = tempfile::Builder::new()
            .prefix("wgic-llm-")
            .tempdir()
            .map_err(|e| RpcError::internal(format!("tempdir create failed: {e}")))?;
        let mut image_names = Vec::with_capacity(images.len());
        for (i, img) in images.iter().enumerate() {
            let name = format!("image-{}{}", i + 1, ext_for_mime(&img.mime_type));
            std::fs::write(dir.path().join(&name), &img.data)
                .map_err(|e| RpcError::internal(format!("image write failed: {e}")))?;
            image_names.push(name);
        }
        Ok(Self { dir, image_names })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" | "image/jpg" => ".jpg",
        "image/webp" => ".webp",
        "image/gif" => ".gif",
        _ => ".png",
    }
}

/// "The attached images are the files image-1.png, ... in the current folder." (empty without images)
pub fn images_note(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    format!(
        "\n\nThe attached images are the files {} in the current folder. Open only these files, and do not mention file names or paths in your answer.",
        names.join(", ")
    )
}

// ============================================================================
// Running the process
// ============================================================================

/// Runs the CLI in `work_dir`, reporting `thinking` / `writing` (once per change) and the answer text as it comes.
pub async fn run_streaming(
    run: CliRun,
    work_dir: &Path,
    parse_line: fn(&str) -> LineEvent,
    progress: &ProgressSender,
) -> Result<String, RpcError> {
    let mut child = spawn(&run, work_dir)?;
    if let (Some(mut stdin), Some(text)) = (child.stdin.take(), run.stdin) {
        stdin.write_all(text.as_bytes()).await.map_err(|e| failed("write the prompt", e))?;
    } // stdin is dropped (closed) here: the CLI sees the end of the prompt
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let stderr = tokio::spawn(async move {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text).await;
        text
    });
    let done = read_lines(child.stdout.take().expect("stdout is piped"), parse_line, progress).await?;
    let status = child.wait().await.map_err(|e| failed("wait for the process", e))?;
    finish(done, status, &stderr.await.unwrap_or_default())
}

fn spawn(run: &CliRun, work_dir: &Path) -> Result<Child, RpcError> {
    let mut command = Command::new(&run.program);
    command
        .args(&run.args)
        .current_dir(work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true); // dropping the call (timeout, cancel) stops the CLI
    command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            RpcError::unavailable(format!("{} is not installed (not on PATH)", run.program))
        }
        _ => failed(&format!("start {}", run.program), e),
    })
}

async fn read_lines(
    stdout: ChildStdout,
    parse_line: fn(&str) -> LineEvent,
    progress: &ProgressSender,
) -> Result<Option<Result<String, RpcError>>, RpcError> {
    let mut lines = BufReader::new(stdout).lines();
    let mut stage = "";
    let mut done = None;
    while let Some(line) = lines.next_line().await.map_err(|e| failed("read the output", e))? {
        match parse_line(&line) {
            LineEvent::Ignore => {}
            LineEvent::Thinking => enter_stage(&mut stage, "thinking", progress),
            LineEvent::Text(text) => {
                enter_stage(&mut stage, "writing", progress);
                progress.text(text);
            }
            LineEvent::Done(result) => done = Some(result),
        }
    }
    Ok(done)
}

fn enter_stage(stage: &mut &'static str, next: &'static str, progress: &ProgressSender) {
    if *stage != next {
        *stage = next;
        progress.report(next);
    }
}

fn failed(what: &str, error: std::io::Error) -> RpcError {
    RpcError::internal(format!("{what}: {error}"))
}

/// The result line wins; without one, a failed exit becomes an error with the end of stderr.
fn finish(done: Option<Result<String, RpcError>>, status: ExitStatus, stderr: &str) -> Result<String, RpcError> {
    if let Some(result) = done {
        return result;
    }
    let tail = last_chars(stderr.trim(), 500);
    if !status.success() {
        return Err(RpcError::internal(format!("exited with {status}: {tail}")));
    }
    Err(RpcError::internal(format!("no result line in the output. stderr: {tail}")))
}

/// The last `max` characters (error output ends with the interesting part).
fn last_chars(text: &str, max: usize) -> &str {
    let skip = text.chars().count().saturating_sub(max);
    text.char_indices().nth(skip).map_or(text, |(index, _)| &text[index..])
}
