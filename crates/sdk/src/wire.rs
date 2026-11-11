//! Messages on a call's reply inbox. Errors are not frames: they are NATS micro error replies
//! (empty payload + `Nats-Service-Error: "<code>: <message>"`), see `parse_service_error`.

use crate::RpcError;

pub const KIND_PROGRESS: u8 = 0x01;
pub const KIND_OK: u8 = 0x02;
pub const KIND_TEXT: u8 = 0x04;

/// Header with the caller's time budget in milliseconds; the server stops the call when it runs out.
pub const TIMEOUT_HEADER: &str = "Wgic-Timeout-Ms";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamFrame {
    /// `0x01` + UTF-8 step (0 or more per call).
    Progress(String),
    /// `0x04` + UTF-8 piece of the answer being written (0 or more per call).
    Text(String),
    /// `0x02` + protobuf response (the last message of a successful call).
    Ok(Vec<u8>),
}

impl StreamFrame {
    pub fn encode(&self) -> Vec<u8> {
        let (kind, body) = match self {
            Self::Progress(text) => (KIND_PROGRESS, text.as_bytes()),
            Self::Text(text) => (KIND_TEXT, text.as_bytes()),
            Self::Ok(data) => (KIND_OK, data.as_slice()),
        };
        let mut buf = Vec::with_capacity(1 + body.len());
        buf.push(kind);
        buf.extend_from_slice(body);
        buf
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RpcError> {
        let (&kind, rest) = bytes.split_first().ok_or_else(|| RpcError::internal("empty stream frame"))?;
        let text = || String::from_utf8_lossy(rest).into_owned();
        match kind {
            KIND_PROGRESS => Ok(Self::Progress(text())),
            KIND_TEXT => Ok(Self::Text(text())),
            KIND_OK => Ok(Self::Ok(rest.to_vec())),
            other => Err(RpcError::internal(format!("unknown frame kind: {other:#x}"))),
        }
    }
}

/// `Nats-Service-Error` text `"deadline_exceeded: no answer within 3s"` → that `RpcError`.
pub fn parse_service_error(text: &str) -> RpcError {
    match text.split_once(": ") {
        Some((code, message)) => RpcError::from_code(code, message),
        None => RpcError::internal(text),
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;
