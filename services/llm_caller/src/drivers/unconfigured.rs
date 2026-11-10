//! The stand-in when `LLM_CALLER_DRIVER` is not set at all.
//!
//! The host builds every service by default, so a host that doesn't use the LLM caller must still
//! start. This instance answers every call with `failed_precondition` and says what to set.

use tokio::sync::mpsc;

use crate::config::LlmConfig;
use crate::domain::{Delta, DriverFuture, LlmDriver, LlmError, LlmRequest};

pub struct Unconfigured;

impl LlmConfig for Unconfigured {
    fn id(&self) -> String {
        "llm".to_string()
    }

    fn driver(&self) -> String {
        "none".to_string()
    }

    fn model(&self) -> String {
        String::new()
    }

    fn effort(&self) -> String {
        String::new()
    }

    fn temperature(&self) -> Option<f64> {
        None
    }
}

impl LlmDriver for Unconfigured {
    fn call(&self, _request: LlmRequest, _deltas: mpsc::UnboundedSender<Delta>) -> DriverFuture {
        let message = "set LLM_CALLER_DRIVER and LLM_CALLER_MODEL for this host".to_string();
        Box::pin(async move { Err(LlmError::NotConfigured(message)) })
    }
}
