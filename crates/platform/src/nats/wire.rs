//! Naming and status-code rules of the NATS wire format. Pure functions, no IO.

use std::collections::HashSet;

pub const CONNECT_CODE: &str = "Connect-Code";
pub(crate) const NATS_SERVICE_ERROR: &str = "Nats-Service-Error";
pub(crate) const NATS_SERVICE_ERROR_CODE: &str = "Nats-Service-Error-Code";

/// Settings for the NATS adapter, usually from the environment.
#[derive(Debug, Clone)]
pub struct NatsConfig {
    /// NATS server URL(s), comma-separated.
    pub url: String,
    /// Subject prefix for all RPC endpoints.
    pub prefix: String,
    /// Micro service version (semver).
    pub version: String,
}

impl NatsConfig {
    /// Reads `NATS_URL` (required), `NATS_PREFIX` (default `rpc`) and
    /// `NATS_SERVICE_VERSION` (default: crate version). `None` if `NATS_URL` is not set.
    pub fn from_env() -> Option<Self> {
        let env = |key: &str, default: &str| std::env::var(key).unwrap_or_else(|_| default.into());
        Some(Self {
            url: std::env::var("NATS_URL").ok()?,
            prefix: env("NATS_PREFIX", "rpc"),
            version: env("NATS_SERVICE_VERSION", env!("CARGO_PKG_VERSION")),
        })
    }
}

/// `greet.v1.GreetService/Greet` → `rpc.greet.v1.GreetService.Greet`
pub fn subject_for(prefix: &str, path: &str) -> String {
    let path = path.trim_start_matches('/').replace('/', ".");
    if prefix.is_empty() { path } else { format!("{prefix}.{path}") }
}

/// Keep `[A-Za-z0-9_-]`, replace anything else with `_` (NATS micro naming rule).
fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// Short endpoint name `GreetService-Greet`, or the fully-qualified
/// `greet_v1_GreetService_Greet` when the short one is taken.
pub fn endpoint_name(path: &str, taken: &mut HashSet<String>) -> String {
    let path = path.trim_start_matches('/');
    let (service, method) = path.rsplit_once('/').unwrap_or(("", path));
    let short_service = service.rsplit('.').next().unwrap_or(service);
    let short = sanitize(&format!("{short_service}-{method}"));
    let name = if taken.contains(&short) { sanitize(&format!("{service}_{method}")) } else { short };
    taken.insert(name.clone());
    name
}

/// Connect's HTTP status → code mapping, for errors from non-connect micro services.
pub fn code_for_status(status: u16) -> &'static str {
    match status {
        400 => "invalid_argument",
        401 => "unauthenticated",
        403 => "permission_denied",
        404 | 501 => "unimplemented",
        408 | 504 => "deadline_exceeded",
        409 => "aborted",
        412 => "failed_precondition",
        413 | 429 => "resource_exhausted",
        499 => "canceled",
        502 | 503 => "unavailable",
        _ => "unknown",
    }
}

/// Connect unary error body `{"code": "...", "message": "..."}` → (code, message).
/// The message falls back to the code, so it is never empty.
pub fn parse_connect_error(body: &[u8]) -> (String, String) {
    let (code, message) = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(v) => {
            (v["code"].as_str().unwrap_or("unknown").to_string(), v["message"].as_str().unwrap_or_default().to_string())
        }
        Err(_) => ("unknown".into(), String::from_utf8_lossy(body).into()),
    };
    if message.is_empty() {
        return (code.clone(), code);
    }
    (code, message)
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
