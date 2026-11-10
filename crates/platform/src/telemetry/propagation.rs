//! W3C trace context (`traceparent`, `tracestate`, `baggage`) in HTTP and NATS headers.

use http::HeaderMap;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::trace::TraceContextExt;
use opentelemetry::{Context, global};
use opentelemetry_http::{HeaderExtractor, HeaderInjector};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// The W3C propagation headers we own; they are rewritten at every hop.
pub const PROPAGATION_HEADERS: [&str; 3] = ["traceparent", "tracestate", "baggage"];

pub fn is_propagation_header(name: &str) -> bool {
    PROPAGATION_HEADERS.iter().any(|h| h.eq_ignore_ascii_case(name))
}

/// Parent context from HTTP headers.
pub fn extract_http(headers: &HeaderMap) -> Context {
    global::get_text_map_propagator(|p| p.extract(&HeaderExtractor(headers)))
}

/// Write `cx` into HTTP headers.
pub fn inject_http(cx: &Context, headers: &mut HeaderMap) {
    global::get_text_map_propagator(|p| p.inject_context(cx, &mut HeaderInjector(headers)));
}

/// Read propagation headers from a NATS message (case-insensitive).
struct NatsHeaderExtractor<'a>(&'a async_nats::HeaderMap);

impl Extractor for NatsHeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| AsRef::<str>::as_ref(*k).eq_ignore_ascii_case(key))
            .and_then(|(_, values)| values.first())
            .map(|v| v.as_str())
    }
    fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(k, _)| AsRef::<str>::as_ref(k)).collect()
    }
}

/// Write propagation headers into a NATS message.
struct NatsHeaderInjector<'a>(&'a mut async_nats::HeaderMap);

impl Injector for NatsHeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key, value.as_str());
    }
}

pub fn extract_nats(headers: Option<&async_nats::HeaderMap>) -> Context {
    match headers {
        Some(h) => global::get_text_map_propagator(|p| p.extract(&NatsHeaderExtractor(h))),
        None => Context::new(),
    }
}

pub fn inject_nats(cx: &Context, headers: &mut async_nats::HeaderMap) {
    global::get_text_map_propagator(|p| p.inject_context(cx, &mut NatsHeaderInjector(headers)));
}

/// Make `parent` the span's parent if it carries a valid remote span context.
pub fn continue_trace(span: &Span, parent: Context) {
    if parent.span().span_context().is_valid() {
        let _ = span.set_parent(parent);
    }
}

/// `/greet.v1.GreetService/Greet` → (`greet.v1.GreetService`, `Greet`)
pub fn split_procedure(path: &str) -> (&str, &str) {
    let p = path.trim_start_matches('/');
    p.rsplit_once('/').unwrap_or(("", p))
}

#[cfg(test)]
#[path = "propagation_tests.rs"]
mod tests;
