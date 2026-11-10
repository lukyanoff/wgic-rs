//! A fake OpenAI Responses API server for e2e tests: hosts run the `openai` llm_caller driver against it,
//! so no API key or internet is needed. It streams a JSON answer in pieces with pauses (so clients see
//! progress), or with `hang` never finishes and notes when the client hangs up (to prove cancellation).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The answer, streamed in three pieces: `{"city":"Paris","country":"FR"}`.
pub const PIECES: [&str; 3] = ["{\"city\":", " \"Paris\",", " \"country\": \"FR\"}"];

/// What the fake server saw: the last request, and whether a client hung up on a hanging answer.
#[derive(Clone, Default)]
pub struct Seen {
    pub request: Arc<Mutex<String>>,
    pub hung_up: Arc<Mutex<bool>>,
}

/// Answer every request with a streamed Responses API reply (or, with `hang`, never finish);
/// return the base URL.
pub async fn fake_openai(hang: bool, seen: Seen) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind fake server");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(answer(socket, hang, seen.clone()));
        }
    });
    format!("http://{addr}")
}

async fn answer(mut socket: TcpStream, hang: bool, seen: Seen) {
    let request = read_request(&mut socket).await;
    *seen.request.lock().expect("lock") = String::from_utf8_lossy(&request).into_owned();
    if hang {
        wait_for_hang_up(socket, seen).await;
        return;
    }
    let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
    let _ = socket.write_all(head.as_bytes()).await;
    for piece in PIECES {
        let event = serde_event("response.output_text.delta", &format!("\"delta\":{piece:?}"));
        let _ = socket.write_all(event.as_bytes()).await;
        tokio::time::sleep(Duration::from_millis(300)).await; // > the service's progress interval
    }
    let usage = "\"response\":{\"usage\":{\"input_tokens\":7,\"output_tokens\":9}}";
    let _ = socket.write_all(serde_event("response.completed", usage).as_bytes()).await;
}

/// Send the response head, then nothing: the call only ends when the client goes away.
async fn wait_for_hang_up(mut socket: TcpStream, seen: Seen) {
    let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n";
    let _ = socket.write_all(head.as_bytes()).await;
    let mut buf = [0u8; 64];
    while let Ok(n) = socket.read(&mut buf).await {
        if n == 0 {
            break;
        }
    }
    *seen.hung_up.lock().expect("lock") = true;
}

fn serde_event(kind: &str, fields: &str) -> String {
    format!("data: {{\"type\":\"{kind}\",{fields}}}\n\n")
}

/// Read until the headers and the `content-length` body are in; return all of it.
async fn read_request(socket: &mut TcpStream) -> Vec<u8> {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let Ok(n) = socket.read(&mut buf).await else { return data };
        if n == 0 {
            return data;
        }
        data.extend_from_slice(&buf[..n]);
        if request_complete(&data) {
            return data;
        }
    }
}

fn request_complete(data: &[u8]) -> bool {
    let text = String::from_utf8_lossy(data);
    let Some(end) = text.find("\r\n\r\n") else {
        return false;
    };
    let mut length = 0;
    for line in text[..end].lines() {
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse::<usize>().unwrap_or(0);
        }
    }
    data.len() >= end + 4 + length
}

impl Seen {
    /// Wait up to `limit` for the client to hang up on a hanging answer.
    pub async fn wait_for_hang_up(&self, limit: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + limit;
        while tokio::time::Instant::now() < deadline {
            if *self.hung_up.lock().expect("lock") {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        false
    }
}
