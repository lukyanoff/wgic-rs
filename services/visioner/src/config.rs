use std::time::Duration;

use serde::{Deserialize, Serialize};

/// `--config` JSON of a visioner instance. Missing fields take the defaults below (`#[serde(default)]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default sensor instance, e.g. `screen` (a request may override it).
    pub sensor: String,
    /// Default llm_caller instance, e.g. `agy` (a request may override it).
    pub llm_caller: String,
    pub system_prompt: String,
    pub text_prompt: String,
    pub user_prompt: String,
    /// A capture that takes longer fails with `deadline_exceeded` (the sensor is told and stops too).
    pub sensor_timeout_secs: u64,
    /// An LLM call that takes longer fails with `deadline_exceeded` (the CLI is killed).
    pub llm_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sensor: String::new(),
            llm_caller: String::new(),
            system_prompt: String::new(),
            text_prompt: String::new(),
            user_prompt: String::new(),
            sensor_timeout_secs: 15,
            llm_timeout_secs: 120,
        }
    }
}

impl Config {
    pub fn sensor_timeout(&self) -> Duration {
        Duration::from_secs(self.sensor_timeout_secs)
    }

    pub fn llm_timeout(&self) -> Duration {
        Duration::from_secs(self.llm_timeout_secs)
    }
}
