//! Unit tests for `config.rs`.

use super::*;

#[test]
fn reads_required_and_default_settings() {
    let deps = Deps::in_process(&[]).with_env([("VISIONER_SCREEN", "screen-1"), ("VISIONER_LLMS", " a, ,b ")]);
    let settings = VisionerSettings::from_env(&deps);
    assert_eq!((settings.screen.as_str(), settings.llms.clone()), ("screen-1", vec!["a".into(), "b".into()]));
    assert_eq!((settings.id.as_str(), settings.llm_timeout), ("vision", secs(120)));
}

#[test]
fn seconds_must_be_in_range() {
    assert_eq!(parse_secs("30"), Some(secs(30)));
    assert_eq!(parse_secs("0"), None);
    assert_eq!(parse_secs("1.5"), None);
}

#[test]
#[should_panic(expected = "VISIONER_SCREEN is not set")]
fn missing_screen_stops_the_host() {
    VisionerSettings::from_env(&Deps::in_process(&[]).with_env([("VISIONER_LLMS", "a"), ("VISIONER_SCREEN", "")]));
}
