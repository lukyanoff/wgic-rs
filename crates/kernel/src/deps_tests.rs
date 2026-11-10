//! Unit tests for `deps.rs`. Routing by instance id over real RPCs is tested in `libs/clients`.

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
