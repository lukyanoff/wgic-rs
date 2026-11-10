//! Visioner settings, read once at startup. Which instances it calls never comes from a request.

use std::time::Duration;

use kernel::Deps;

/// ≈ a C# options class bound from configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionerSettings {
    /// This instance, e.g. `vision-1` (logs).
    pub id: String,
    /// The sensor instance to capture from, e.g. `screen-1`.
    pub screen: String,
    /// The llm_caller instances to ask, in order, e.g. `["llm-claude", "llm-agy"]`.
    pub llms: Vec<String>,
    pub screen_timeout: Duration,
    pub llm_timeout: Duration,
}

impl VisionerSettings {
    /// `VISIONER_SCREEN` and `VISIONER_LLMS` (comma list) are required; the rest have defaults.
    pub fn from_env(deps: &Deps) -> Self {
        let llms = parse_list(&deps.required("VISIONER_LLMS", "llm instance ids, e.g. llm-claude,llm-agy"));
        if llms.is_empty() {
            panic!("VISIONER_LLMS has no instance ids, expected e.g. llm-claude,llm-agy");
        }
        Self {
            id: deps.setting("VISIONER_ID", "vision".into(), "an instance id", |v| Some(v.to_string())),
            screen: deps.required("VISIONER_SCREEN", "a sensor instance id, e.g. screen-1"),
            llms,
            screen_timeout: deps.setting("VISIONER_SCREEN_TIMEOUT_SECS", secs(15), "seconds, 1..3600", parse_secs),
            llm_timeout: deps.setting("VISIONER_LLM_TIMEOUT_SECS", secs(120), "seconds, 1..3600", parse_secs),
        }
    }
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// `"a, b,,c"` → `["a", "b", "c"]`
fn parse_list(value: &str) -> Vec<String> {
    value.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// Whole seconds, 1..=3600.
fn parse_secs(value: &str) -> Option<Duration> {
    let n: u64 = value.parse().ok()?;
    (1..=3600).contains(&n).then(|| secs(n))
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
