//! Unit tests for `config.rs`.

use std::collections::HashMap;

use super::*;

fn settings(pairs: &[(&str, &str)]) -> Result<VisionerSettings, ConfigError> {
    let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    VisionerSettings::from_lookup(&|key| map.get(key).cloned())
}

#[test]
fn screen_and_llms_with_defaults() {
    let s = settings(&[(SCREEN_ENV, "screen-1"), (LLMS_ENV, "llm-claude, llm-agy,")]).unwrap();
    assert_eq!((s.id.as_str(), s.screen.as_str()), ("vision", "screen-1"));
    assert_eq!(s.llms, ["llm-claude", "llm-agy"]);
    assert_eq!((s.screen_timeout, s.llm_timeout), (Duration::from_secs(15), Duration::from_secs(120)));
}

#[test]
fn screen_and_llms_are_required() {
    assert!(settings(&[(LLMS_ENV, "llm-a")]).unwrap_err().0.contains(SCREEN_ENV));
    assert!(settings(&[(SCREEN_ENV, "screen-1"), (LLMS_ENV, " , ")]).unwrap_err().0.contains(LLMS_ENV));
}

#[test]
fn timeouts_are_whole_seconds_in_range() {
    let base = [(SCREEN_ENV, "s"), (LLMS_ENV, "l")];
    let ok = settings(&[base[0], base[1], (LLM_TIMEOUT_ENV, "3")]).unwrap();
    assert_eq!(ok.llm_timeout, Duration::from_secs(3));
    assert!(settings(&[base[0], base[1], (SCREEN_TIMEOUT_ENV, "0")]).is_err());
    assert!(settings(&[base[0], base[1], (LLM_TIMEOUT_ENV, "1.5")]).is_err());
}
