//! Unit tests for `host.rs`.

use super::*;
use connectrpc::Router;

struct Named(&'static str);

impl Module for Named {
    fn name(&self) -> &'static str {
        self.0
    }
    fn register(&self, router: Router) -> Router {
        router
    }
}

fn modules() -> Modules {
    vec![Arc::new(Named("greet")), Arc::new(Named("billing"))]
}

fn names(modules: &[Arc<dyn Module>]) -> Vec<&'static str> {
    modules.iter().map(|m| m.name()).collect()
}

#[test]
fn empty_or_all_selects_everything() {
    assert_eq!(names(&select_modules(modules(), "").unwrap()), ["greet", "billing"]);
    assert_eq!(names(&select_modules(modules(), "all").unwrap()), ["greet", "billing"]);
}

#[test]
fn selects_named_modules() {
    let selected = select_modules(modules(), " billing ").unwrap();
    assert_eq!(names(&selected), ["billing"]);
}

#[test]
fn unknown_module_lists_available_ones() {
    let error = select_modules(modules(), "greet,nope").err().unwrap();
    assert_eq!(error.to_string(), "unknown service 'nope'; available: greet, billing");
}
