//! Visioner settings, read once at startup from env. Which instances it calls never comes from a request.

use std::fmt;
use std::time::Duration;

use kernel::Deps;

pub const ID_ENV: &str = "VISIONER_ID";
pub const SCREEN_ENV: &str = "VISIONER_SCREEN";
pub const LLMS_ENV: &str = "VISIONER_LLMS";
pub const SCREEN_TIMEOUT_ENV: &str = "VISIONER_SCREEN_TIMEOUT_SECS";
pub const LLM_TIMEOUT_ENV: &str = "VISIONER_LLM_TIMEOUT_SECS";

/// One instance's settings (≈ a C# options class bound from configuration).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionerSettings {
    /// This instance, e.g. `vision-1` (logs, metrics).
    pub id: String,
    /// The screen sensor instance to capture from, e.g. `screen-1`.
    pub screen: String,
    /// The llm_caller instances to ask, in order, e.g. `["llm-claude", "llm-agy"]`.
    pub llms: Vec<String>,
    pub screen_timeout: Duration,
    pub llm_timeout: Duration,
}

/// Reads a setting by name; `deps.env` in production, a map in tests.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl VisionerSettings {
    pub fn from_env(deps: &Deps) -> Result<Self, ConfigError> {
        Self::from_lookup(&|key| deps.env(key))
    }

    /// `VISIONER_SCREEN` and `VISIONER_LLMS` (comma-separated) are required; the rest have defaults.
    pub fn from_lookup(get: Lookup) -> Result<Self, ConfigError> {
        let Some(screen) = get(SCREEN_ENV).map(|s| s.trim().to_string()) else {
            return Err(ConfigError(format!("{SCREEN_ENV} is not set (the screen sensor instance id, e.g. screen-1)")));
        };
        let llms = parse_list(&get(LLMS_ENV).unwrap_or_default());
        if llms.is_empty() {
            return Err(ConfigError(format!("{LLMS_ENV} is not set (llm instance ids, e.g. llm-claude,llm-agy)")));
        }
        Ok(Self {
            id: get(ID_ENV).unwrap_or_else(|| "vision".into()),
            screen,
            llms,
            screen_timeout: parse_secs(get, SCREEN_TIMEOUT_ENV, 15)?,
            llm_timeout: parse_secs(get, LLM_TIMEOUT_ENV, 120)?,
        })
    }
}

/// `"a, b,,c"` → `["a", "b", "c"]`
fn parse_list(value: &str) -> Vec<String> {
    value.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// Whole seconds, 1..=3600; `default` when unset.
fn parse_secs(get: Lookup, key: &str, default: u64) -> Result<Duration, ConfigError> {
    let Some(value) = get(key) else {
        return Ok(Duration::from_secs(default));
    };
    match value.trim().parse::<u64>() {
        Ok(secs) if (1..=3600).contains(&secs) => Ok(Duration::from_secs(secs)),
        _ => Err(ConfigError(format!("{key}={value:?} must be whole seconds, 1..3600"))),
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
