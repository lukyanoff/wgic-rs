//! Unit tests for `propagation.rs`.

use super::*;
use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};
use opentelemetry_sdk::propagation::TraceContextPropagator;

#[test]
fn splits_procedure_paths() {
    assert_eq!(
        split_procedure("/greet.v1.GreetService/Greet"),
        ("greet.v1.GreetService", "Greet")
    );
    assert_eq!(split_procedure("Greet"), ("", "Greet"));
}

#[test]
fn propagation_headers_are_case_insensitive() {
    assert!(is_propagation_header("TraceParent"));
    assert!(!is_propagation_header("content-type"));
}

#[test]
fn trace_context_round_trips_through_nats_headers() {
    global::set_text_map_propagator(TraceContextPropagator::new());
    let trace_id = TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736").unwrap();
    let span_context = SpanContext::new(
        trace_id,
        SpanId::from_hex("00f067aa0ba902b7").unwrap(),
        TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    );
    let cx = Context::new().with_remote_span_context(span_context);

    let mut headers = async_nats::HeaderMap::new();
    inject_nats(&cx, &mut headers);
    let extracted = extract_nats(Some(&headers));

    assert_eq!(extracted.span().span_context().trace_id(), trace_id);
}
