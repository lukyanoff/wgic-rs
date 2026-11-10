use std::fmt;

use opentelemetry::trace::TraceContextExt;
use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Span, Subscriber};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

use super::log_backend::SERVICE_FIELDS;

/// Service logs carry their fields as one JSON string (see `log_backend::SERVICE_FIELDS`);
/// put them back at the top level. Never overwrites the standard keys.
fn flatten_service_fields(out: &mut Map<String, Value>) {
    let Some(Value::String(json)) = out.remove(SERVICE_FIELDS) else {
        return;
    };
    let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(&json) else {
        out.insert(SERVICE_FIELDS.into(), json.into());
        return;
    };
    for (key, value) in fields {
        out.entry(key).or_insert(value);
    }
}

/// One JSON object per line:
/// `{"timestamp","level","target","message",...fields,"span","trace_id","span_id"}`.
pub struct JsonLogFormat;

#[derive(Default)]
struct JsonFields(Map<String, Value>);

impl Visit for JsonFields {
    fn record_str(&mut self, f: &Field, v: &str) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_i64(&mut self, f: &Field, v: i64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_u64(&mut self, f: &Field, v: u64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_f64(&mut self, f: &Field, v: f64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_bool(&mut self, f: &Field, v: bool) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_error(&mut self, f: &Field, v: &(dyn std::error::Error + 'static)) {
        self.0.insert(f.name().into(), v.to_string().into());
    }
    fn record_debug(&mut self, f: &Field, v: &dyn fmt::Debug) {
        self.0.insert(f.name().into(), format!("{v:?}").into());
    }
}

impl<S, N> FormatEvent<S, N> for JsonLogFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let mut timestamp = String::new();
        SystemTime.format_time(&mut Writer::new(&mut timestamp))?;

        let mut fields = JsonFields::default();
        event.record(&mut fields);

        let mut out = Map::new();
        out.insert("timestamp".into(), timestamp.into());
        out.insert("level".into(), meta.level().as_str().into());
        out.insert("target".into(), meta.target().into());
        if let Some(message) = fields.0.remove("message") {
            out.insert("message".into(), message);
        }
        out.extend(fields.0);
        flatten_service_fields(&mut out);

        if let Some(span) = ctx.lookup_current() {
            out.insert("span".into(), span.name().into());
        }
        let span_context = Span::current().context().span().span_context().clone();
        if span_context.is_valid() {
            out.insert("trace_id".into(), span_context.trace_id().to_string().into());
            out.insert("span_id".into(), span_context.span_id().to_string().into());
        }
        let line = serde_json::to_string(&Value::Object(out)).map_err(|_| fmt::Error)?;
        writeln!(writer, "{line}")
    }
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
