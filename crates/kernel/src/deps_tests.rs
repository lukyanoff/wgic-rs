//! Unit tests for `deps.rs`.

use std::sync::Mutex;

use super::*;

/// Remembers which instance ids were asked for.
#[derive(Default)]
struct RecordingFactory {
    asked: Arc<Mutex<Vec<String>>>,
}

impl ChannelFactory for RecordingFactory {
    fn channel_to(&self, instance: &str) -> Channel {
        self.asked.lock().expect("lock").push(instance.to_string());
        Channel::in_process(Router::new())
    }
}

#[test]
fn channel_to_asks_the_factory_for_that_instance() {
    let factory = RecordingFactory::default();
    let asked = factory.asked.clone();
    let deps = Deps::in_process(&[]).with_channels(factory);
    deps.channel_to("screen-1");
    deps.channel_to("llm-claude");
    assert_eq!(*asked.lock().expect("lock"), ["screen-1", "llm-claude"]);
}

fn deps() -> Deps {
    Deps::in_process(&[]).with_env([("KERNEL_TEST_NUMBER", " 7 "), ("KERNEL_TEST_BLANK", "  ")])
}

#[test]
fn env_is_trimmed_and_blank_means_unset() {
    assert_eq!(deps().env("KERNEL_TEST_NUMBER").as_deref(), Some("7"));
    assert_eq!(deps().env("KERNEL_TEST_BLANK"), None);
}

#[test]
fn setting_parses_the_value_or_uses_the_default() {
    let parse = |v: &str| v.parse::<u32>().ok();
    assert_eq!(deps().setting("KERNEL_TEST_NUMBER", 1, "a number", parse), 7);
    assert_eq!(deps().setting("KERNEL_TEST_MISSING", 1, "a number", parse), 1);
}

#[test]
#[should_panic(expected = "KERNEL_TEST_NUMBER=\"7\" is invalid, expected yes or no")]
fn bad_setting_stops_the_host() {
    deps().setting("KERNEL_TEST_NUMBER", true, "yes or no", |v| match v {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    });
}

#[test]
#[should_panic(expected = "KERNEL_TEST_MISSING is not set, expected a name")]
fn missing_required_setting_stops_the_host() {
    deps().required("KERNEL_TEST_MISSING", "a name");
}
