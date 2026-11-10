//! `CliDriver`: the base class of the CLI drivers (claude, agy).
//!
//! Starts the program in its own empty folder, writes the prompt to stdin, reads one JSON object per stdout
//! line and lets the flavor (the "subclass") say what each line means. The process is killed when the call
//! is dropped: the client went away or its deadline passed.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use connectrpc::ConnectError;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use super::{CallRequest, ChatMessage, Delta, Deltas, DriverFuture, Image, LlmDriver, LlmReply};
use crate::config::LlmSettings;

// ---------------------------------------------------------------------------------------------
// The abstract part: what differs per CLI
// ---------------------------------------------------------------------------------------------

/// What one stdout line means.
#[derive(Debug)]
pub enum LineEvent {
    /// Nothing interesting (init, status, ...).
    Ignore,
    Delta(Delta),
    /// The last line: the whole answer, or the CLI's own error report.
    Done(Result<LlmReply, ConnectError>),
}

/// ≈ the `abstract` members of `CliDriverBase`.
pub trait CliFlavor: Send + Sync + 'static {
    /// The program, e.g. `claude` (`LLM_CALLER_CLI_PATH` replaces it).
    fn program(&self) -> &'static str;
    fn args(&self, settings: &LlmSettings, request: &CallRequest) -> Vec<String>;
    /// Text for stdin; `None` = stdin stays empty (the prompt is in the args).
    fn stdin(&self, request: &CallRequest) -> Option<String>;
    fn parse_line(&self, line: &str) -> LineEvent;
    /// Files written into the call's folder before the program starts: (name, bytes). Default: none.
    fn files(&self, _request: &CallRequest) -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------------------------
// The base class body
// ---------------------------------------------------------------------------------------------

/// Cheap to clone (an `Arc` + settings), so every call's future can own its copy.
#[derive(Clone)]
pub struct CliDriver {
    flavor: Arc<dyn CliFlavor>,
    settings: LlmSettings,
}

impl CliDriver {
    pub fn new(flavor: impl CliFlavor, settings: &LlmSettings) -> Self {
        Self { flavor: Arc::new(flavor), settings: settings.clone() }
    }

    async fn run(self, request: CallRequest, deltas: Deltas) -> Result<LlmReply, ConnectError> {
        let work_dir = WorkDir::create(self.flavor.files(&request))?; // deleted when this call ends
        let mut child = self.spawn(&request, &work_dir)?;
        if let (Some(mut stdin), Some(text)) = (child.stdin.take(), self.flavor.stdin(&request)) {
            stdin.write_all(text.as_bytes()).await.map_err(|e| failed("write the prompt", e))?;
        } // stdin is dropped (closed) here: the CLI sees the end of the prompt
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let stderr = tokio::spawn(async move {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text).await;
            text
        });

        let mut lines = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();
        let mut done = None;
        while let Some(line) = lines.next_line().await.map_err(|e| failed("read the output", e))? {
            match self.flavor.parse_line(&line) {
                LineEvent::Ignore => {}
                LineEvent::Delta(delta) => {
                    let _ = deltas.send(delta);
                }
                LineEvent::Done(result) => done = Some(result),
            }
        }
        let status = child.wait().await.map_err(|e| failed("wait for the process", e))?;
        finish(done, status, &stderr.await.unwrap_or_default())
    }

    fn spawn(&self, request: &CallRequest, work_dir: &WorkDir) -> Result<Child, ConnectError> {
        let program = self.settings.cli_path.clone().unwrap_or_else(|| self.flavor.program().to_string());
        let mut command = Command::new(&program);
        command
            .args(self.flavor.args(&self.settings, request))
            .current_dir(&work_dir.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true); // dropping the call (cancel, deadline) stops the CLI
        match command.spawn() {
            Ok(child) => Ok(child),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(ConnectError::failed_precondition(format!("{program} is not installed (not found on PATH)")))
            }
            Err(e) => Err(failed(&format!("start {program}"), e)),
        }
    }
}

impl LlmDriver for CliDriver {
    fn call(&self, request: CallRequest, deltas: Deltas) -> DriverFuture {
        Box::pin(self.clone().run(request, deltas))
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// The call's own folder: empty except for the flavor's files, so an agent-style CLI finds no project files
/// to look at. Deleted when dropped (≈ C# `IDisposable`), also on cancel.
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn create(files: Vec<(String, Vec<u8>)>) -> Result<WorkDir, ConnectError> {
        static NEXT: AtomicU64 = AtomicU64::new(0); // ≈ a C# `static long` used with `Interlocked.Increment`
        let name = format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let work_dir = WorkDir { path: std::env::temp_dir().join("llm_caller").join(name) };
        std::fs::create_dir_all(&work_dir.path).map_err(|e| failed("create the work folder", e))?;
        for (name, bytes) in files {
            std::fs::write(work_dir.path.join(&name), bytes).map_err(|e| failed(&format!("write {name}"), e))?;
        }
        Ok(work_dir)
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn failed(what: &str, error: std::io::Error) -> ConnectError {
    ConnectError::internal(format!("{what}: {error}"))
}

/// The result line wins; without one, a failed exit becomes an error with the end of stderr.
fn finish(
    done: Option<Result<LlmReply, ConnectError>>,
    status: ExitStatus,
    stderr: &str,
) -> Result<LlmReply, ConnectError> {
    if let Some(result) = done {
        return result;
    }
    let tail = last_chars(stderr.trim(), 500);
    if !status.success() {
        return Err(ConnectError::internal(format!("exited with {status}: {tail}")));
    }
    Err(ConnectError::internal(format!("no result line in the output. stderr: {tail}")))
}

/// The last `max` characters (error output ends with the interesting part).
fn last_chars(text: &str, max: usize) -> &str {
    let skip = text.chars().count().saturating_sub(max);
    match text.char_indices().nth(skip) {
        Some((index, _)) => &text[index..],
        None => text,
    }
}

// ---------------------------------------------------------------------------------------------
// Rendering the chat for a CLI (pure, shared by the flavors)
// ---------------------------------------------------------------------------------------------

/// All system messages, joined by blank lines.
pub fn system_prompt(messages: &[ChatMessage]) -> String {
    let parts: Vec<&str> = messages.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect();
    parts.join("\n\n")
}

/// The other messages as one prompt. A single message is passed as it is; a longer chat becomes a
/// `User:` / `Assistant:` transcript that ends with the last turn.
pub fn transcript(messages: &[ChatMessage]) -> String {
    let chat: Vec<&ChatMessage> = messages.iter().filter(|m| m.role != "system").collect();
    if let [only] = chat.as_slice() {
        return only.content.clone();
    }
    let mut text = String::from("Continue this conversation. Reply as the assistant to the last user turn.\n");
    for message in chat {
        let speaker = if message.role == "user" { "User" } else { "Assistant" };
        text.push_str(&format!("\n{speaker}: {}\n", message.content));
    }
    text
}

/// Every image in the chat with a file name: `image-1.png`, `image-2.jpg`, ...
pub fn image_files(messages: &[ChatMessage]) -> Vec<(String, &Image)> {
    let mut files = Vec::new();
    for image in messages.iter().flat_map(|m| &m.images) {
        let extension = image.mime_type.strip_prefix("image/").unwrap_or("png").replace("jpeg", "jpg");
        files.push((format!("image-{}.{extension}", files.len() + 1), image));
    }
    files
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
