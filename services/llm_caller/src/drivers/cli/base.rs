//! `CliCaller`: the shared "base class" of the CLI drivers (claude, agy).
//!
//! Starts the program in print mode with stream-json output, feeds it the prompt, reads one JSON
//! object per stdout line and hands each line to the flavor (the "subclass") to interpret.
//! The child process is killed when the call is dropped (client went away) or times out.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::mpsc;

use crate::config::LlmSettings;
use crate::domain::{ChatMessage, Delta, DriverFuture, Image, LlmDriver, LlmError, LlmReply, LlmRequest, Role};

// ---------------------------------------------------------------------------------------------
// Interface: what differs per CLI (≈ the abstract members of `CliCallerBase`)
// ---------------------------------------------------------------------------------------------

/// What one stdout line means.
#[derive(Debug, Clone, PartialEq)]
pub enum LineEvent {
    /// Nothing interesting (init, status, ...).
    Ignore,
    Delta(Delta),
    /// The final line: the full answer, or the CLI's own error report.
    Done(Result<LlmReply, LlmError>),
}

pub trait CliFlavor: Send + Sync {
    /// Default program name, e.g. `claude`.
    fn program(&self) -> &'static str;
    /// Command-line arguments for one call.
    fn args(&self, settings: &LlmSettings, request: &LlmRequest) -> Vec<String>;
    /// Text written to stdin; `None` = stdin stays empty (the prompt is in `args`).
    fn stdin(&self, request: &LlmRequest) -> Option<String>;
    fn parse_line(&self, line: &str) -> LineEvent;
    /// Files written into the call's own work folder before the CLI starts, as (name, bytes).
    /// Default: none. agy gets the images this way, because it reads pictures only from files.
    fn files(&self, _request: &LlmRequest) -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------------------------
// The base class body
// ---------------------------------------------------------------------------------------------

/// Cheap to clone (an `Arc` + settings), so each call's future can own a copy.
#[derive(Clone)]
pub struct CliCaller {
    flavor: Arc<dyn CliFlavor>,
    settings: LlmSettings,
}

impl CliCaller {
    pub fn new(flavor: Arc<dyn CliFlavor>, settings: LlmSettings) -> Self {
        Self { flavor, settings }
    }

    async fn run(self, request: LlmRequest, deltas: mpsc::UnboundedSender<Delta>) -> Result<LlmReply, LlmError> {
        match tokio::time::timeout(self.settings.timeout, self.run_process(request, deltas)).await {
            Ok(result) => result,
            Err(_) => Err(LlmError::Unavailable(format!("no answer within {:?}", self.settings.timeout))),
        }
    }

    async fn run_process(
        &self,
        request: LlmRequest,
        deltas: mpsc::UnboundedSender<Delta>,
    ) -> Result<LlmReply, LlmError> {
        let work_dir = WorkDir::create(self.flavor.files(&request))?; // deleted when this call ends
        let mut child = self.spawn(&request, work_dir.path())?;
        write_stdin(&mut child, self.flavor.stdin(&request)).await?;
        let stderr = tokio::spawn(read_all(child.stderr.take()));
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut lines = BufReader::new(stdout).lines();
        let mut done = None;
        while let Some(line) = lines.next_line().await.map_err(|e| LlmError::Failed(format!("read output: {e}")))? {
            match self.flavor.parse_line(&line) {
                LineEvent::Ignore => {}
                LineEvent::Delta(delta) => {
                    let _ = deltas.send(delta);
                }
                LineEvent::Done(result) => done = Some(result),
            }
        }
        let status = child.wait().await.map_err(|e| LlmError::Failed(format!("wait for process: {e}")))?;
        let stderr = stderr.await.unwrap_or_default();
        finish(done, status, &stderr)
    }

    fn spawn(&self, request: &LlmRequest, work_dir: &Path) -> Result<Child, LlmError> {
        let program = self.settings.cli_path.clone().unwrap_or_else(|| self.flavor.program().to_string());
        let mut command = Command::new(&program);
        command
            .args(self.flavor.args(&self.settings, request))
            .current_dir(work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true); // dropping the call (cancel, timeout) stops the CLI
        match command.spawn() {
            Ok(child) => Ok(child),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(LlmError::NotConfigured(format!("{program} is not installed (not found on PATH)")))
            }
            Err(e) => Err(LlmError::Failed(format!("start {program}: {e}"))),
        }
    }
}

impl LlmDriver for CliCaller {
    fn call(&self, request: LlmRequest, deltas: mpsc::UnboundedSender<Delta>) -> DriverFuture {
        Box::pin(self.clone().run(request, deltas))
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// The call's own folder to run in: empty except for the flavor's files, so an agent-style CLI finds no
/// project files to look at. Deleted when dropped (≈ C# `IDisposable`), also on cancel and timeout.
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn create(files: Vec<(String, Vec<u8>)>) -> Result<WorkDir, LlmError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let path = std::env::temp_dir().join("llm_caller").join(name);
        let work_dir = WorkDir { path };
        std::fs::create_dir_all(&work_dir.path).map_err(|e| LlmError::Failed(format!("create work folder: {e}")))?;
        for (name, bytes) in files {
            std::fs::write(work_dir.path.join(&name), bytes)
                .map_err(|e| LlmError::Failed(format!("write {name}: {e}")))?;
        }
        Ok(work_dir)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

async fn write_stdin(child: &mut Child, text: Option<String>) -> Result<(), LlmError> {
    let Some(mut stdin) = child.stdin.take() else {
        return Ok(());
    };
    if let Some(text) = text {
        stdin.write_all(text.as_bytes()).await.map_err(|e| LlmError::Failed(format!("write prompt: {e}")))?;
    }
    Ok(()) // dropping stdin closes it: the CLI sees the end of the prompt
}

async fn read_all(stderr: Option<ChildStderr>) -> String {
    let mut text = String::new();
    if let Some(mut stderr) = stderr {
        let _ = stderr.read_to_string(&mut text).await;
    }
    text
}

/// The result line wins; without one, a failed exit becomes an error with the stderr tail.
pub fn finish(
    done: Option<Result<LlmReply, LlmError>>,
    status: ExitStatus,
    stderr: &str,
) -> Result<LlmReply, LlmError> {
    if let Some(result) = done {
        return result;
    }
    let tail = last_chars(stderr.trim(), 500);
    if !status.success() {
        return Err(LlmError::Failed(format!("exited with {status}: {tail}")));
    }
    Err(LlmError::Failed(format!("no result line in the output. stderr: {tail}")))
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
// Rendering the chat for a CLI (pure, unit tested)
// ---------------------------------------------------------------------------------------------

/// All system messages, joined by blank lines.
pub fn system_prompt(messages: &[ChatMessage]) -> String {
    let parts: Vec<&str> = messages.iter().filter(|m| m.role == Role::System).map(|m| m.content.as_str()).collect();
    parts.join("\n\n")
}

/// The non-system messages as one prompt. A single user message is passed as it is;
/// a longer chat becomes a `User:` / `Assistant:` transcript that ends with the last turn.
pub fn transcript(messages: &[ChatMessage]) -> String {
    let chat: Vec<&ChatMessage> = messages.iter().filter(|m| m.role != Role::System).collect();
    if let [only] = chat.as_slice() {
        return only.content.clone();
    }
    let mut text = String::from("Continue this conversation. Reply as the assistant to the last user turn.\n");
    for message in chat {
        let speaker = if message.role == Role::User { "User" } else { "Assistant" };
        text.push_str(&format!("\n{speaker}: {}\n", message.content));
    }
    text
}

/// Every image in the chat with a file name: `image-1.png`, `image-2.jpg`, ...
pub fn image_files(messages: &[ChatMessage]) -> Vec<(String, &Image)> {
    let mut files = Vec::new();
    for image in messages.iter().flat_map(|m| m.images.iter()) {
        let extension = match image.mime_type.as_str() {
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            _ => "png",
        };
        files.push((format!("image-{}.{extension}", files.len() + 1), image));
    }
    files
}

#[cfg(test)]
#[path = "base_tests.rs"]
mod tests;
