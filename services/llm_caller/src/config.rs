//! Instance settings: which driver, model, effort, temperature. Read once at startup and never
//! changed by a request (one running instance = one model setup).
//!
//! `LlmConfig` is the interface the base class sees (≈ C# `ILlmOptions`); `LlmSettings` is the
//! plain implementation (≈ a POCO options class) that also carries driver-only settings.

// ---------------------------------------------------------------------------------------------
// Usings + constants
// ---------------------------------------------------------------------------------------------

use std::fmt;
use std::time::Duration;

use kernel::Deps;

pub const ID_ENV: &str = "LLM_CALLER_ID";
pub const DRIVER_ENV: &str = "LLM_CALLER_DRIVER";
pub const MODEL_ENV: &str = "LLM_CALLER_MODEL";
pub const EFFORT_ENV: &str = "LLM_CALLER_EFFORT";
pub const TEMPERATURE_ENV: &str = "LLM_CALLER_TEMPERATURE";
pub const TIMEOUT_ENV: &str = "LLM_CALLER_TIMEOUT_SECS";
pub const BASE_URL_ENV: &str = "LLM_CALLER_BASE_URL";
pub const CLI_PATH_ENV: &str = "LLM_CALLER_CLI_PATH";

const CLI_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
const HTTP_EFFORTS: &[&str] = &["minimal", "low", "medium", "high"];

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

/// Which LLM caller this instance is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverKind {
    /// `claude -p` (Claude Code CLI).
    ClaudeCli,
    /// `agy -p` (Antigravity CLI).
    AgyCli,
    /// OpenAI Responses API over HTTP.
    OpenAi,
    /// OpenRouter chat completions over HTTP.
    OpenRouter,
}

/// What the shared caller logic needs to know (≈ C# `interface ILlmConfig`).
pub trait LlmConfig: Send + Sync {
    /// Instance id, e.g. `llm-claude`. Shows up in metadata, logs and metrics.
    fn id(&self) -> String;
    /// Driver name, e.g. `claude-cli`.
    fn driver(&self) -> String;
    fn model(&self) -> String;
    fn effort(&self) -> String;
    /// `None` = the provider's default (and the only option for the CLI drivers).
    fn temperature(&self) -> Option<f64>;
}

/// Plain settings object, filled from env by `from_env`.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmSettings {
    pub id: String,
    pub driver: DriverKind,
    pub model: String,
    pub effort: String,
    pub temperature: Option<f64>,
    /// The whole call (process or HTTP request) is cancelled after this.
    pub timeout: Duration,
    /// HTTP drivers: replaces `https://api.openai.com` / `https://openrouter.ai` (tests, proxies).
    pub base_url: Option<String>,
    /// CLI drivers: replaces the `claude` / `agy` program (tests).
    pub cli_path: Option<String>,
}

/// Reads one setting by key (≈ C# `IConfiguration[key]`); `None` = not set.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// A setting that could not be parsed. Reported at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub key: &'static str,
    pub value: String,
    pub expected: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={:?} is invalid, expected {}", self.key, self.value, self.expected)
    }
}

fn invalid(key: &'static str, value: &str, expected: &str) -> ConfigError {
    ConfigError { key, value: value.to_string(), expected: expected.to_string() }
}

// ---------------------------------------------------------------------------------------------
// Reading settings
// ---------------------------------------------------------------------------------------------

impl LlmSettings {
    /// Settings with defaults: effort `medium`, no temperature, 300 s timeout.
    pub fn new(id: &str, driver: DriverKind, model: &str) -> Self {
        Self {
            id: id.to_string(),
            driver,
            model: model.to_string(),
            effort: "medium".to_string(),
            temperature: None,
            timeout: Duration::from_secs(300),
            base_url: None,
            cli_path: None,
        }
    }

    pub fn from_env(deps: &Deps) -> Result<Self, ConfigError> {
        Self::from_lookup(&|key: &str| deps.env(key))
    }

    /// Read every `LLM_CALLER_*` value through `get`, so tests can fake the environment.
    pub fn from_lookup(get: Lookup) -> Result<Self, ConfigError> {
        let driver = parse_driver(&get(DRIVER_ENV).unwrap_or_default())?;
        let Some(model) = get(MODEL_ENV) else {
            return Err(invalid(MODEL_ENV, "", "a model name, e.g. claude-sonnet-5-5"));
        };
        let id = get(ID_ENV).unwrap_or_else(|| "llm".to_string());
        let mut settings = Self::new(id.trim(), driver, model.trim());
        if let Some(value) = get(EFFORT_ENV) {
            settings.effort = parse_effort(driver, &value)?;
        }
        if let Some(value) = get(TEMPERATURE_ENV) {
            settings.temperature = Some(parse_temperature(driver, &value)?);
        }
        if let Some(value) = get(TIMEOUT_ENV) {
            settings.timeout = parse_timeout(&value)?;
        }
        settings.base_url = get(BASE_URL_ENV).map(|url| url.trim().trim_end_matches('/').to_string());
        settings.cli_path = get(CLI_PATH_ENV).map(|path| path.trim().to_string());
        Ok(settings)
    }
}

impl LlmConfig for LlmSettings {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn driver(&self) -> String {
        self.driver.name().to_string()
    }

    fn model(&self) -> String {
        self.model.clone()
    }

    fn effort(&self) -> String {
        self.effort.clone()
    }

    fn temperature(&self) -> Option<f64> {
        self.temperature
    }
}

impl DriverKind {
    pub fn name(self) -> &'static str {
        match self {
            DriverKind::ClaudeCli => "claude-cli",
            DriverKind::AgyCli => "agy-cli",
            DriverKind::OpenAi => "openai",
            DriverKind::OpenRouter => "openrouter",
        }
    }

    /// The CLIs have no temperature flag.
    pub fn supports_temperature(self) -> bool {
        matches!(self, DriverKind::OpenAi | DriverKind::OpenRouter)
    }

    fn efforts(self) -> &'static [&'static str] {
        if self.supports_temperature() { HTTP_EFFORTS } else { CLI_EFFORTS }
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing (pure functions, unit tested)
// ---------------------------------------------------------------------------------------------

/// `claude-cli` | `agy-cli` | `openai` | `openrouter`
pub fn parse_driver(value: &str) -> Result<DriverKind, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "claude-cli" => Ok(DriverKind::ClaudeCli),
        "agy-cli" => Ok(DriverKind::AgyCli),
        "openai" => Ok(DriverKind::OpenAi),
        "openrouter" => Ok(DriverKind::OpenRouter),
        _ => Err(invalid(DRIVER_ENV, value, "claude-cli, agy-cli, openai or openrouter")),
    }
}

/// CLIs: `low|medium|high|xhigh|max`; HTTP APIs: `minimal|low|medium|high`.
pub fn parse_effort(driver: DriverKind, value: &str) -> Result<String, ConfigError> {
    let effort = value.trim().to_ascii_lowercase();
    if driver.efforts().contains(&effort.as_str()) {
        return Ok(effort);
    }
    Err(invalid(EFFORT_ENV, value, &driver.efforts().join(", ")))
}

/// `0..=2`, only for drivers that support it.
pub fn parse_temperature(driver: DriverKind, value: &str) -> Result<f64, ConfigError> {
    if !driver.supports_temperature() {
        return Err(invalid(TEMPERATURE_ENV, value, &format!("no value: {} has no temperature", driver.name())));
    }
    match value.trim().parse::<f64>() {
        Ok(temperature) if (0.0..=2.0).contains(&temperature) => Ok(temperature),
        _ => Err(invalid(TEMPERATURE_ENV, value, "a number 0..2")),
    }
}

/// Seconds, `1..=3600`.
pub fn parse_timeout(value: &str) -> Result<Duration, ConfigError> {
    match value.trim().parse::<u64>() {
        Ok(seconds @ 1..=3600) => Ok(Duration::from_secs(seconds)),
        _ => Err(invalid(TIMEOUT_ENV, value, "seconds 1..3600")),
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
