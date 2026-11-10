//! Unit tests for `config.rs`.

use std::collections::HashMap;

use super::*;

fn settings(pairs: &[(&str, &str)]) -> Result<LlmSettings, ConfigError> {
    let env: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    LlmSettings::from_lookup(&|key| env.get(key).cloned())
}

#[test]
fn reads_all_settings() {
    let s = settings(&[
        (ID_ENV, "llm-openai"),
        (DRIVER_ENV, "openai"),
        (MODEL_ENV, "gpt-x"),
        (EFFORT_ENV, "LOW"),
        (TEMPERATURE_ENV, "0.2"),
        (TIMEOUT_ENV, "60"),
        (BASE_URL_ENV, "http://localhost:9/"),
    ])
    .unwrap();
    assert_eq!((s.id.as_str(), s.driver, s.model.as_str()), ("llm-openai", DriverKind::OpenAi, "gpt-x"));
    assert_eq!((s.effort.as_str(), s.temperature), ("low", Some(0.2)));
    assert_eq!(s.timeout, Duration::from_secs(60));
    assert_eq!(s.base_url.as_deref(), Some("http://localhost:9"));
}

#[test]
fn defaults_are_medium_effort_and_no_temperature() {
    let s = settings(&[(DRIVER_ENV, "claude-cli"), (MODEL_ENV, "sonnet")]).unwrap();
    assert_eq!((s.id.as_str(), s.effort.as_str(), s.temperature), ("llm", "medium", None));
}

#[test]
fn driver_and_model_are_required() {
    assert_eq!(settings(&[(MODEL_ENV, "m")]).unwrap_err().key, DRIVER_ENV);
    assert_eq!(settings(&[(DRIVER_ENV, "openai")]).unwrap_err().key, MODEL_ENV);
}

#[test]
fn temperature_is_rejected_for_cli_drivers() {
    let error = settings(&[(DRIVER_ENV, "agy-cli"), (MODEL_ENV, "m"), (TEMPERATURE_ENV, "0.5")]).unwrap_err();
    assert_eq!(error.key, TEMPERATURE_ENV);
    assert!(error.expected.contains("agy-cli has no temperature"));
}

#[test]
fn temperature_must_be_0_to_2() {
    assert!(parse_temperature(DriverKind::OpenRouter, "2.5").is_err());
    assert!(parse_temperature(DriverKind::OpenRouter, "warm").is_err());
}

#[test]
fn effort_values_depend_on_driver() {
    assert_eq!(parse_effort(DriverKind::ClaudeCli, "max").unwrap(), "max");
    assert!(parse_effort(DriverKind::OpenAi, "max").is_err());
    assert_eq!(parse_effort(DriverKind::OpenAi, "minimal").unwrap(), "minimal");
    assert!(parse_effort(DriverKind::AgyCli, "minimal").is_err());
}

#[test]
fn timeout_must_be_1_to_3600_seconds() {
    assert!(parse_timeout("0").is_err());
    assert_eq!(parse_timeout("3600").unwrap(), Duration::from_secs(3600));
}
