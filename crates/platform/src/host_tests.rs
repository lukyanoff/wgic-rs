//! Unit tests for `host.rs`.

use super::*;
use connectrpc::Router;

struct Greet;

impl Module for Greet {
    fn name(&self) -> &'static str {
        "greet"
    }
    fn register(&self, router: Router) -> Router {
        router
    }
}

fn greet(_: &Deps) -> Arc<dyn Module> {
    Arc::new(Greet)
}

fn billing(_: &Deps) -> Arc<dyn Module> {
    panic!("billing must not be built unless SERVICES names it");
}

const ALL: [(&str, ModuleFactory); 2] = [("greet", greet), ("billing", billing)];

fn build(wanted: &str) -> anyhow::Result<Modules> {
    build_modules(&ALL, wanted, &Deps::in_process(&[]))
}

#[test]
fn builds_only_the_named_services() {
    let modules = build(" greet ").unwrap();
    let names: Vec<&str> = modules.iter().map(|m| m.name()).collect();
    assert_eq!(names, ["greet"]);
}

#[test]
fn services_must_be_named() {
    let error = build("").err().unwrap();
    assert_eq!(error.to_string(), "set SERVICES to a comma list of: greet, billing");
}

#[test]
fn unknown_service_lists_available_ones() {
    let error = build("greet,nope").err().unwrap();
    assert_eq!(error.to_string(), "unknown service 'nope'; available: greet, billing");
}
