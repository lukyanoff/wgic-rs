//! Call a service over NATS: `NatsRpcClient` (generic) + typed adapters and registries for the contracts' traits.
//!
//! One call: publish the request with `traceparent` and `Wgic-Timeout-Ms` headers, then read the reply inbox,
//! passing progress on, until the `0x02` answer, a micro error reply, "no responders", or the timeout.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_nats::service::NATS_SERVICE_ERROR;
use async_nats::{HeaderMap, StatusCode, Subscriber};
use async_trait::async_trait;
use contracts::{
    ClientRegistry, ILlmCallerService, ISensorService, LlmCallRequest, LlmCallResponse, ProgressSender, RpcError,
    SensorCaptureRequest, SensorCaptureResponse,
};
use futures::StreamExt;
use prost::Message;
use tracing::field::Empty;
use tracing::{Instrument, Span};

use crate::server::{CALL_DEADLINE, rpc_subject};
use crate::telemetry;
use crate::wire::{StreamFrame, TIMEOUT_HEADER, parse_service_error};

// ============================================================================
// Generic client
// ============================================================================

/// Calls one instance (`<module>/<instance>`). Every call gets `timeout` (or less, when the call being served has
/// less time left); the server is told via `Wgic-Timeout-Ms`, so it stops too.
#[derive(Clone)]
pub struct NatsRpcClient {
    nc: async_nats::Client,
    module: String,
    instance: String,
    timeout: Duration,
}

impl NatsRpcClient {
    pub fn new(
        nc: async_nats::Client,
        module: impl Into<String>,
        instance: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self { nc, module: module.into(), instance: instance.into(), timeout }
    }

    pub async fn call<Req, Res>(
        &self,
        service: &str,
        method: &str,
        req: &Req,
        progress: &ProgressSender,
    ) -> Result<Res, RpcError>
    where
        Req: Message,
        Res: Message + Default,
    {
        let span = tracing::info_span!(
            "rpc.client",
            otel.name = %format!("{service}/{method}"),
            otel.kind = "client",
            otel.status_code = Empty,
            otel.status_message = Empty,
            rpc.system = "nats",
            rpc.service = service,
            rpc.method = method,
            wgic.target = %format!("{}/{}", self.module, self.instance),
        );
        let subject = rpc_subject(&self.module, &self.instance, service, method);
        let result = self.send(&subject, req.encode_to_vec(), progress).instrument(span.clone()).await;
        if let Err(e) = &result {
            span.record("otel.status_code", "ERROR");
            span.record("otel.status_message", e.to_string().as_str());
            // Logged inside the client span, so the line carries `wgic.target` (which downstream call failed).
            span.in_scope(|| tracing::warn!(error.type = e.code, error.message = %e.message, "rpc call failed"));
        }
        Res::decode(result?.as_slice()).map_err(|e| RpcError::internal(format!("decode error: {e}")))
    }

    /// This client's timeout, or less when the call we are serving has less time left.
    fn budget(&self) -> Duration {
        let left = CALL_DEADLINE.try_with(|deadline| deadline.saturating_duration_since(Instant::now()));
        left.map_or(self.timeout, |left| left.min(self.timeout))
    }

    async fn send(&self, subject: &str, payload: Vec<u8>, progress: &ProgressSender) -> Result<Vec<u8>, RpcError> {
        let budget = self.budget();
        let inbox = self.nc.new_inbox();
        let mut replies = self.nc.subscribe(inbox.clone()).await.map_err(|e| RpcError::unavailable(e.to_string()))?;
        let mut headers = HeaderMap::new();
        headers.insert(TIMEOUT_HEADER, budget.as_millis().to_string().as_str());
        telemetry::inject(&Span::current(), &mut headers);
        self.nc
            .publish_with_reply_and_headers(subject.to_string(), inbox, headers, payload.into())
            .await
            .map_err(|e| RpcError::unavailable(e.to_string()))?;

        let answer = tokio::time::timeout(budget, read_answer(&mut replies, subject, progress)).await;
        let _ = replies.unsubscribe().await;
        answer.unwrap_or_else(|_| {
            let secs = budget.as_secs_f32();
            Err(RpcError::deadline_exceeded(format!("{}/{}: no answer within {secs:.1}s", self.module, self.instance)))
        })
    }
}

/// Reads the reply inbox until the call is done.
async fn read_answer(replies: &mut Subscriber, subject: &str, progress: &ProgressSender) -> Result<Vec<u8>, RpcError> {
    while let Some(msg) = replies.next().await {
        if let Some(answer) = on_reply(msg, subject, progress)? {
            return Ok(answer);
        }
    }
    Err(RpcError::unavailable(format!("reply stream for {subject} closed")))
}

/// One inbox message: `Some(answer)` when the call succeeded, `None` for progress.
fn on_reply(msg: async_nats::Message, subject: &str, progress: &ProgressSender) -> Result<Option<Vec<u8>>, RpcError> {
    if msg.status == Some(StatusCode::NO_RESPONDERS) {
        return Err(RpcError::unavailable(format!("nothing is listening on {subject}")));
    }
    if let Some(error) = msg.headers.as_ref().and_then(|h| h.get(NATS_SERVICE_ERROR)) {
        return Err(parse_service_error(error.as_str()));
    }
    match StreamFrame::decode(&msg.payload)? {
        StreamFrame::Progress(step) => progress.report(step),
        StreamFrame::Text(delta) => progress.text(delta),
        StreamFrame::Ok(answer) => return Ok(Some(answer)),
    }
    Ok(None)
}

// ============================================================================
// Typed adapters & registries implementing the `contracts` traits
// ============================================================================

pub struct NatsSensorClient(pub NatsRpcClient);

#[async_trait]
impl ISensorService for NatsSensorClient {
    async fn capture(
        &self,
        req: SensorCaptureRequest,
        progress: ProgressSender,
    ) -> Result<SensorCaptureResponse, RpcError> {
        self.0.call("sensor.v1.SensorService", "Capture", &req, &progress).await
    }
}

pub struct NatsLlmCallerClient(pub NatsRpcClient);

#[async_trait]
impl ILlmCallerService for NatsLlmCallerClient {
    async fn call(&self, req: LlmCallRequest, progress: ProgressSender) -> Result<LlmCallResponse, RpcError> {
        self.0.call("llm_caller.v1.LlmCallerService", "Call", &req, &progress).await
    }
}

/// `get("screen")` → a client for `sensor/screen` whose calls time out after `timeout`.
pub struct NatsSensorRegistry {
    nc: async_nats::Client,
    timeout: Duration,
}

impl NatsSensorRegistry {
    pub fn new(nc: async_nats::Client, timeout: Duration) -> Self {
        Self { nc, timeout }
    }
}

impl ClientRegistry<dyn ISensorService> for NatsSensorRegistry {
    fn get(&self, target: &str) -> Result<Arc<dyn ISensorService>, RpcError> {
        let instance = target.strip_prefix("sensor/").unwrap_or(target);
        Ok(Arc::new(NatsSensorClient(NatsRpcClient::new(self.nc.clone(), "sensor", instance, self.timeout))))
    }
}

/// `get("agy")` → a client for `llm_caller/agy` whose calls time out after `timeout`.
pub struct NatsLlmRegistry {
    nc: async_nats::Client,
    timeout: Duration,
}

impl NatsLlmRegistry {
    pub fn new(nc: async_nats::Client, timeout: Duration) -> Self {
        Self { nc, timeout }
    }
}

impl ClientRegistry<dyn ILlmCallerService> for NatsLlmRegistry {
    fn get(&self, target: &str) -> Result<Arc<dyn ILlmCallerService>, RpcError> {
        let instance = target.strip_prefix("llm_caller/").unwrap_or(target);
        Ok(Arc::new(NatsLlmCallerClient(NatsRpcClient::new(self.nc.clone(), "llm_caller", instance, self.timeout))))
    }
}
