use crate::RpcError;

pub const KIND_PROGRESS: u8 = 0x01;
pub const KIND_OK: u8 = 0x02;
pub const KIND_ERR: u8 = 0x03;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamFrame {
    Progress(String),
    Ok(Vec<u8>),
    Err(RpcError),
}

impl StreamFrame {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Progress(msg) => {
                let mut buf = Vec::with_capacity(1 + msg.len());
                buf.push(KIND_PROGRESS);
                buf.extend_from_slice(msg.as_bytes());
                buf
            }
            Self::Ok(data) => {
                let mut buf = Vec::with_capacity(1 + data.len());
                buf.push(KIND_OK);
                buf.extend_from_slice(data);
                buf
            }
            Self::Err(err) => {
                let body = format!("{}: {}", err.code, err.message);
                let mut buf = Vec::with_capacity(1 + body.len());
                buf.push(KIND_ERR);
                buf.extend_from_slice(body.as_bytes());
                buf
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RpcError> {
        let (&kind, rest) = bytes
            .split_first()
            .ok_or_else(|| RpcError::internal("empty stream frame"))?;
        match kind {
            KIND_PROGRESS => Ok(Self::Progress(String::from_utf8_lossy(rest).into_owned())),
            KIND_OK => Ok(Self::Ok(rest.to_vec())),
            KIND_ERR => {
                let text = String::from_utf8_lossy(rest);
                let (code, msg) = text.split_once(": ").unwrap_or(("internal", &text));
                let static_code = match code {
                    "invalid_argument" => "invalid_argument",
                    "not_found" => "not_found",
                    _ => "internal",
                };
                Ok(Self::Err(RpcError {
                    code: static_code,
                    message: msg.to_string(),
                }))
            }
            other => Err(RpcError::internal(format!("unknown frame kind: {other:#x}"))),
        }
    }
}
