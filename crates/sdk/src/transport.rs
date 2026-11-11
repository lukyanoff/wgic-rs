use crate::wire::StreamFrame;
use async_trait::async_trait;
use contracts::{
    ClientRegistry, ILlmCallerService, ISensorService, LlmCallRequest, LlmCallResponse, ProgressSender, RawRpcEndpoint,
    RpcError, SensorCaptureRequest, SensorCaptureResponse,
};
use futures::StreamExt;
use prost::Message;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub fn rpc_subject(module: &str, instance: &str, service: &str, method: &str) -> String {
    format!("rpc.{module}.{instance}.{service}.{method}")
}

/// Shared NATS RPC Client helper used by service client adapters.
#[derive(Clone)]
pub struct NatsRpcClient {
    nc: async_nats::Client,
    module: String,
    instance: String,
    timeout: Duration,
}

impl NatsRpcClient {
    pub fn new(nc: async_nats::Client, module: impl Into<String>, instance: impl Into<String>) -> Self {
        Self { nc, module: module.into(), instance: instance.into(), timeout: Duration::from_secs(60) }
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
        let subject = rpc_subject(&self.module, &self.instance, service, method);
        let inbox = self.nc.new_inbox();
        let mut sub = self.nc.subscribe(inbox.clone()).await.map_err(|e| RpcError::internal(e.to_string()))?;

        let payload = req.encode_to_vec();
        self.nc
            .publish_with_reply(subject, inbox, payload.into())
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?;

        let deadline = tokio::time::sleep(self.timeout);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                _ = &mut deadline => return Err(RpcError::internal("rpc deadline exceeded")),
                maybe_msg = sub.next() => {
                    let msg = maybe_msg.ok_or_else(|| RpcError::internal("subscription closed"))?;
                    match StreamFrame::decode(&msg.payload)? {
                        StreamFrame::Progress(step) => progress.report(step),
                        StreamFrame::Ok(bytes) => {
                            let _ = sub.unsubscribe().await;
                            return Res::decode(bytes.as_slice())
                                .map_err(|e| RpcError::internal(format!("decode error: {e}")));
                        }
                        StreamFrame::Err(err) => {
                            let _ = sub.unsubscribe().await;
                            return Err(err);
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Concrete NATS Client Adapters & Registries implementing `contracts` Traits
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

pub struct NatsSensorRegistry {
    nc: async_nats::Client,
}
impl NatsSensorRegistry {
    pub fn new(nc: async_nats::Client) -> Self {
        Self { nc }
    }
}
impl ClientRegistry<dyn ISensorService> for NatsSensorRegistry {
    fn get(&self, target: &str) -> Result<Arc<dyn ISensorService>, RpcError> {
        let instance = target.strip_prefix("sensor/").unwrap_or(target);
        Ok(Arc::new(NatsSensorClient(NatsRpcClient::new(self.nc.clone(), "sensor", instance))))
    }
}

pub struct NatsLlmRegistry {
    nc: async_nats::Client,
}
impl NatsLlmRegistry {
    pub fn new(nc: async_nats::Client) -> Self {
        Self { nc }
    }
}
impl ClientRegistry<dyn ILlmCallerService> for NatsLlmRegistry {
    fn get(&self, target: &str) -> Result<Arc<dyn ILlmCallerService>, RpcError> {
        let instance = target.strip_prefix("llm_caller/").unwrap_or(target);
        Ok(Arc::new(NatsLlmCallerClient(NatsRpcClient::new(self.nc.clone(), "llm_caller", instance))))
    }
}

/// Serves any `RawRpcEndpoint` over NATS with Queue Group load-balancing for High Availability.
pub async fn serve_endpoint(
    nc: async_nats::Client,
    module: &str,
    instance: &str,
    endpoint: Arc<dyn RawRpcEndpoint>,
) -> anyhow::Result<Vec<tokio::task::JoinHandle<()>>> {
    let mut handles = Vec::new();
    let queue_group = format!("q.{module}.{instance}");

    for &method in endpoint.methods() {
        let subject = rpc_subject(module, instance, endpoint.service_name(), method);
        let mut sub = nc.queue_subscribe(subject.clone(), queue_group.clone()).await?;
        let nc_clone = nc.clone();
        let ep_clone = endpoint.clone();
        let method_name = method.to_string();

        tracing::info!(subject = %subject, queue = %queue_group, "listening on NATS RPC");

        let handle = tokio::spawn(async move {
            while let Some(msg) = sub.next().await {
                tokio::spawn(handle_request(nc_clone.clone(), ep_clone.clone(), method_name.clone(), msg));
            }
        });
        handles.push(handle);
    }
    Ok(handles)
}

/// Runs one RPC call: streams progress frames to the reply inbox, then the terminal Ok/Err frame.
async fn handle_request(
    nc: async_nats::Client,
    endpoint: Arc<dyn RawRpcEndpoint>,
    method: String,
    msg: async_nats::Message,
) {
    let Some(reply) = msg.reply.clone() else { return };
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let nc_prog = nc.clone();
    let reply_prog = reply.clone();

    let prog_task = tokio::spawn(async move {
        while let Some(step) = rx.recv().await {
            let frame = StreamFrame::Progress(step).encode();
            let _ = nc_prog.publish(reply_prog.clone(), frame.into()).await;
        }
    });

    let res = endpoint.dispatch(&method, &msg.payload, ProgressSender::new(tx)).await;
    let _ = prog_task.await;

    let final_frame = match res {
        Ok(bytes) => StreamFrame::Ok(bytes).encode(),
        Err(err) => StreamFrame::Err(err).encode(),
    };
    // A failed terminal publish (e.g. a payload over the NATS 1 MB limit) would leave the caller waiting forever,
    // so tell it why with a small Err frame instead.
    if let Err(e) = nc.publish(reply.clone(), final_frame.into()).await {
        tracing::error!(method = %method, error = %e, "failed to publish RPC reply");
        let err = RpcError::internal(format!("reply could not be sent: {e}"));
        let _ = nc.publish(reply, StreamFrame::Err(err).encode().into()).await;
    }
}
