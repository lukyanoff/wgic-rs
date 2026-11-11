//! Serve any `RawRpcEndpoint` as a NATS micro service (`nats micro ls | info | stats | ping` see it).
//!
//! One call:
//! 1. a `rpc.server` span starts, continuing the caller's trace from its `traceparent` header
//! 2. the handler runs within the caller's `Wgic-Timeout-Ms` (capped at [`MAX_CALL`]); when time runs out the
//!    handler future is dropped, which cancels its work (e.g. kills a running LLM CLI)
//! 3. progress goes to the reply inbox while the handler runs; answer text is batched every 250 ms
//! 4. the last reply is `0x02`+protobuf, or a NATS micro error (`Nats-Service-Error: "<code>: <message>"`)
//! 5. the span gets its status, and the `rpc.server.call.duration` histogram one measurement

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_nats::service::{Request, ServiceExt, endpoint::Endpoint, error::Error as ServiceError};
use async_nats::{HeaderMap, Subject};
use bytes::Bytes;
use contracts::{ProgressEvent, ProgressSender, RawRpcEndpoint, RpcError};
use futures::StreamExt;
use opentelemetry::metrics::Histogram;
use opentelemetry::{KeyValue, global};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tracing::field::Empty;
use tracing::{Instrument, Span};

use crate::telemetry;
use crate::wire::{StreamFrame, TIMEOUT_HEADER};

/// No call runs longer than this, even when the caller set no timeout.
pub const MAX_CALL: Duration = Duration::from_secs(600);

tokio::task_local! {
    /// When the call being handled must be done (≈ C# `AsyncLocal<DateTime>`). Calls the handler makes to other
    /// services get at most the time left (see `client.rs`), so the caller's deadline reaches every hop.
    pub(crate) static CALL_DEADLINE: Instant;
}

/// Answer text is sent at most this often, so a fast model doesn't send one message per token.
const TEXT_FLUSH_EVERY: Duration = Duration::from_millis(250);

/// Histogram buckets in seconds: 1 ms … 5 min (LLM calls are slow).
const SECONDS_BUCKETS: [f64; 16] =
    [0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0];

pub fn rpc_subject(module: &str, instance: &str, service: &str, method: &str) -> String {
    format!("rpc.{module}.{instance}.{service}.{method}")
}

// ============================================================================
// Startup
// ============================================================================

/// Starts the micro service `<module>` (metadata `instance`) with one endpoint per method on
/// `rpc.<module>.<instance>.<Service>.<Method>`. Replicas of one instance share the queue group
/// `q.<module>.<instance>`, so NATS load-balances between them.
pub async fn serve_endpoint(
    nc: async_nats::Client,
    module: &str,
    instance: &str,
    endpoint: Arc<dyn RawRpcEndpoint>,
) -> anyhow::Result<Vec<tokio::task::JoinHandle<()>>> {
    let service = nc
        .service_builder()
        .description(format!("{} ({module}/{instance})", endpoint.service_name()))
        .metadata(HashMap::from([("instance".to_string(), instance.to_string())]))
        .start(module, env!("CARGO_PKG_VERSION"))
        .await
        .map_err(|e| anyhow!("start NATS micro service '{module}': {e}"))?;
    let queue_group = format!("q.{module}.{instance}");
    let duration = call_duration_histogram();

    let mut loops = Vec::new();
    for &method in endpoint.methods() {
        let subject = rpc_subject(module, instance, endpoint.service_name(), method);
        let requests = service
            .endpoint_builder()
            .name(endpoint_name(endpoint.service_name(), method))
            .queue_group(&queue_group)
            .add(&subject)
            .await
            .map_err(|e| anyhow!("add NATS endpoint {subject}: {e}"))?;
        tracing::info!(%subject, queue = %queue_group, "listening on NATS RPC");
        let route = Route {
            nc: nc.clone(),
            endpoint: endpoint.clone(),
            method,
            instance: instance.into(),
            duration: duration.clone(),
        };
        loops.push(serve_requests(requests, route));
    }
    // One task owns the micro service (it answers `$SRV.*`) and runs every endpoint loop.
    let task = tokio::spawn(async move {
        let _service = service;
        futures::future::join_all(loops).await;
    });
    Ok(vec![task])
}

/// `sensor.v1.SensorService` + `Capture` → `SensorService-Capture` (micro names allow `[A-Za-z0-9_-]`).
fn endpoint_name(service: &str, method: &str) -> String {
    let short = service.rsplit('.').next().unwrap_or(service);
    format!("{short}-{method}")
}

fn call_duration_histogram() -> Histogram<f64> {
    global::meter("wgic")
        .f64_histogram("rpc.server.call.duration")
        .with_unit("s")
        .with_description("Duration of RPC calls handled by this server")
        .with_boundaries(SECONDS_BUCKETS.to_vec())
        .build()
}

// ============================================================================
// One call
// ============================================================================

/// What every request of one endpoint needs (cheap to clone: `Arc`s and handles).
#[derive(Clone)]
struct Route {
    nc: async_nats::Client,
    endpoint: Arc<dyn RawRpcEndpoint>,
    method: &'static str,
    instance: Arc<str>,
    duration: Histogram<f64>,
}

async fn serve_requests(mut requests: Endpoint, route: Route) {
    while let Some(request) = requests.next().await {
        let span = route.server_span(request.message.headers.as_ref());
        tokio::spawn(route.clone().handle(request).instrument(span));
    }
}

impl Route {
    fn server_span(&self, headers: Option<&HeaderMap>) -> Span {
        let service = self.endpoint.service_name();
        let span = tracing::info_span!(
            "rpc.server",
            otel.name = %format!("{service}/{}", self.method),
            otel.kind = "server",
            otel.status_code = Empty,
            otel.status_message = Empty,
            rpc.system = "nats",
            rpc.service = service,
            rpc.method = self.method,
            wgic.instance = %self.instance,
            rpc.response.status_code = Empty,
        );
        telemetry::continue_trace(&span, headers);
        span
    }

    async fn handle(self, request: Request) {
        let started = Instant::now();
        let limit = timeout_of(request.message.headers.as_ref());
        let (tx, rx) = mpsc::unbounded_channel();
        let forward =
            request.message.reply.clone().map(|reply| tokio::spawn(forward_progress(self.nc.clone(), reply, rx)));

        let call = self.endpoint.dispatch(self.method, &request.message.payload, ProgressSender::new(tx));
        let result = match tokio::time::timeout(limit, CALL_DEADLINE.scope(started + limit, call)).await {
            Ok(result) => result,
            Err(_) => Err(RpcError::deadline_exceeded(format!("no answer within {:.1}s", limit.as_secs_f32()))),
        };
        // The handler (and every `ProgressSender` it held) is gone now, so the forwarder flushes and ends.
        if let Some(forward) = forward {
            let _ = forward.await;
        }
        self.record(&result, started);
        respond(&request, result).await;
    }

    /// Span status + log line + one histogram measurement.
    fn record(&self, result: &Result<Vec<u8>, RpcError>, started: Instant) {
        let code = result.as_ref().err().map_or("ok", |e| e.code);
        let span = Span::current();
        span.record("rpc.response.status_code", code);
        let duration_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(_) => tracing::info!(duration_ms, "rpc finished"),
            Err(e) => {
                span.record("otel.status_code", "ERROR");
                span.record("otel.status_message", e.message.as_str());
                tracing::warn!(duration_ms, error.type = code, error.message = %e.message, "rpc failed");
            }
        }
        let attributes = [
            KeyValue::new("rpc.service", self.endpoint.service_name()),
            KeyValue::new("rpc.method", self.method),
            KeyValue::new("rpc.response.status_code", code),
        ];
        // `service.name` and `wgic.instance` come from the resource (Prometheus labels `service_name`, `wgic_instance`).
        self.duration.record(started.elapsed().as_secs_f64(), &attributes);
    }
}

/// The caller's `Wgic-Timeout-Ms`, capped at `MAX_CALL` (which is also the default).
fn timeout_of(headers: Option<&HeaderMap>) -> Duration {
    let millis = headers.and_then(|h| h.get(TIMEOUT_HEADER)).and_then(|v| v.as_str().parse::<u64>().ok());
    millis.map_or(MAX_CALL, |ms| Duration::from_millis(ms).min(MAX_CALL))
}

/// The last reply: `0x02`+protobuf, or a micro error reply (so `nats micro stats` counts it).
async fn respond(request: &Request, result: Result<Vec<u8>, RpcError>) {
    let reply = match result {
        Ok(bytes) => Ok(Bytes::from(StreamFrame::Ok(bytes).encode())),
        Err(err) => Err(service_error(&err)),
    };
    // A failed reply (e.g. over the NATS 1 MB max_payload) would leave the caller waiting, so say why instead.
    if let Err(e) = request.respond(reply).await {
        tracing::error!(error.message = %e, "failed to publish RPC reply");
        let err = RpcError::internal(format!("reply could not be sent: {e}"));
        let _ = request.respond(Err(service_error(&err))).await;
    }
}

fn service_error(err: &RpcError) -> ServiceError {
    ServiceError { status: err.to_string(), code: err.status() as usize }
}

// ============================================================================
// Progress forwarding
// ============================================================================

/// Publishes the handler's progress to the reply inbox until the handler is done: steps at once, answer text
/// batched (every `TEXT_FLUSH_EVERY`, before the next step, and at the end).
async fn forward_progress(nc: async_nats::Client, reply: Subject, mut events: mpsc::UnboundedReceiver<ProgressEvent>) {
    let mut out = ProgressOut { nc, reply, pending_text: String::new() };
    let mut tick = tokio::time::interval(TEXT_FLUSH_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break };
                out.on_event(event).await;
            }
            _ = tick.tick() => out.flush_text().await,
        }
    }
    out.flush_text().await;
}

struct ProgressOut {
    nc: async_nats::Client,
    reply: Subject,
    pending_text: String,
}

impl ProgressOut {
    async fn on_event(&mut self, event: ProgressEvent) {
        match event {
            ProgressEvent::Text(delta) => self.pending_text.push_str(&delta),
            ProgressEvent::Step(step) => {
                self.flush_text().await;
                self.publish(StreamFrame::Progress(step)).await;
            }
        }
    }

    async fn flush_text(&mut self) {
        if !self.pending_text.is_empty() {
            let text = std::mem::take(&mut self.pending_text);
            self.publish(StreamFrame::Text(text)).await;
        }
    }

    async fn publish(&self, frame: StreamFrame) {
        let _ = self.nc.publish(self.reply.clone(), frame.encode().into()).await;
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod server_tests;
