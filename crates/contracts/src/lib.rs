use async_trait::async_trait;
use prost::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

// ============================================================================
// 1. Foundational RPC & DI Abstractions (Zero Infrastructure Dependencies)
// ============================================================================

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[error("{code}: {message}")]
pub struct RpcError {
    pub code: &'static str,
    pub message: String,
}

impl RpcError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self { code: "internal", message: msg.into() }
    }
    pub fn invalid_arg(msg: impl Into<String>) -> Self {
        Self { code: "invalid_argument", message: msg.into() }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { code: "not_found", message: msg.into() }
    }
}

/// Passed into every RPC handler method. Streams real-time progress updates if subscribed.
#[derive(Clone, Default)]
pub struct ProgressSender {
    tx: Option<mpsc::UnboundedSender<String>>,
}

impl ProgressSender {
    pub fn new(tx: mpsc::UnboundedSender<String>) -> Self {
        Self { tx: Some(tx) }
    }

    pub fn noop() -> Self {
        Self { tx: None }
    }

    pub fn report(&self, message: impl Into<String>) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(message.into());
        }
    }
}

/// Generic SOLID factory/registry interface injected into service constructors.
pub trait ClientRegistry<T: ?Sized + Send + Sync>: Send + Sync {
    fn get(&self, instance_id: &str) -> Result<Arc<T>, RpcError>;
}

/// In-memory registry implementation for fast unit testing and static maps.
pub struct MapRegistry<T: ?Sized> {
    clients: HashMap<String, Arc<T>>,
}

impl<T: ?Sized + Send + Sync> MapRegistry<T> {
    pub fn new(clients: HashMap<String, Arc<T>>) -> Self {
        Self { clients }
    }
}

impl<T: ?Sized + Send + Sync> ClientRegistry<T> for MapRegistry<T> {
    fn get(&self, instance_id: &str) -> Result<Arc<T>, RpcError> {
        self.clients
            .get(instance_id)
            .cloned()
            .ok_or_else(|| RpcError::not_found(format!("instance '{}' not found", instance_id)))
    }
}

/// Transport-agnostic RPC endpoint dispatcher trait.
#[async_trait]
pub trait RawRpcEndpoint: Send + Sync + 'static {
    fn service_name(&self) -> &'static str;
    fn methods(&self) -> &'static [&'static str];
    async fn dispatch(
        &self,
        method: &str,
        payload: &[u8],
        progress: ProgressSender,
    ) -> Result<Vec<u8>, RpcError>;
}

// ============================================================================
// 2. Domain DTOs & Service Interfaces (Protobuf + Pure Traits)
// ============================================================================

// --- Sensor Contract ---
#[derive(Clone, PartialEq, Message)]
pub struct SensorCaptureRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct SensorCaptureResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub data: Vec<u8>,
    #[prost(string, tag = "2")]
    pub mime_type: String,
}

#[async_trait]
pub trait ISensorService: Send + Sync + 'static {
    async fn capture(
        &self,
        req: SensorCaptureRequest,
        progress: ProgressSender,
    ) -> Result<SensorCaptureResponse, RpcError>;
}

// --- LlmCaller Contract ---
#[derive(Clone, PartialEq, Message)]
pub struct ImageInput {
    #[prost(bytes = "vec", tag = "1")]
    pub data: Vec<u8>,
    #[prost(string, tag = "2")]
    pub mime_type: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct LlmCallRequest {
    #[prost(string, tag = "1")]
    pub system_prompt: String,
    #[prost(string, tag = "2")]
    pub text_prompt: String,
    #[prost(string, tag = "3")]
    pub user_prompt: String,
    #[prost(message, repeated, tag = "4")]
    pub images: Vec<ImageInput>,
}

#[derive(Clone, PartialEq, Message)]
pub struct LlmCallResponse {
    #[prost(string, tag = "1")]
    pub text: String,
    #[prost(string, tag = "2")]
    pub model: String,
}

#[async_trait]
pub trait ILlmCallerService: Send + Sync + 'static {
    async fn call(
        &self,
        req: LlmCallRequest,
        progress: ProgressSender,
    ) -> Result<LlmCallResponse, RpcError>;
}

// --- Visioner Contract ---
#[derive(Clone, PartialEq, Message)]
pub struct VisionCaptureRequest {
    #[prost(string, optional, tag = "1")]
    pub sensor: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub llm_caller: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub system_prompt: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub text_prompt: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub user_prompt: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VisionCaptureResponse {
    #[prost(string, tag = "1")]
    pub text: String,
    #[prost(string, tag = "2")]
    pub model: String,
    #[prost(string, tag = "3")]
    pub sensor_mime_type: String,
}

#[async_trait]
pub trait IVisionerService: Send + Sync + 'static {
    async fn capture(
        &self,
        req: VisionCaptureRequest,
        progress: ProgressSender,
    ) -> Result<VisionCaptureResponse, RpcError>;
}

// ============================================================================
// 3. Pure Protobuf Endpoint Dispatchers (Zero NATS dependency)
// ============================================================================

pub struct SensorEndpoint<T: ISensorService>(pub Arc<T>);

#[async_trait]
impl<T: ISensorService> RawRpcEndpoint for SensorEndpoint<T> {
    fn service_name(&self) -> &'static str { "sensor.v1.SensorService" }
    fn methods(&self) -> &'static [&'static str] { &["Capture"] }

    async fn dispatch(
        &self,
        method: &str,
        payload: &[u8],
        progress: ProgressSender,
    ) -> Result<Vec<u8>, RpcError> {
        match method {
            "Capture" => {
                let req = SensorCaptureRequest::decode(payload)
                    .map_err(|e| RpcError::invalid_arg(e.to_string()))?;
                let res = self.0.capture(req, progress).await?;
                Ok(res.encode_to_vec())
            }
            _ => Err(RpcError::not_found(format!("unknown method {method}"))),
        }
    }
}

pub struct LlmCallerEndpoint<T: ILlmCallerService>(pub Arc<T>);

#[async_trait]
impl<T: ILlmCallerService> RawRpcEndpoint for LlmCallerEndpoint<T> {
    fn service_name(&self) -> &'static str { "llm_caller.v1.LlmCallerService" }
    fn methods(&self) -> &'static [&'static str] { &["Call"] }

    async fn dispatch(
        &self,
        method: &str,
        payload: &[u8],
        progress: ProgressSender,
    ) -> Result<Vec<u8>, RpcError> {
        match method {
            "Call" => {
                let req = LlmCallRequest::decode(payload)
                    .map_err(|e| RpcError::invalid_arg(e.to_string()))?;
                let res = self.0.call(req, progress).await?;
                Ok(res.encode_to_vec())
            }
            _ => Err(RpcError::not_found(format!("unknown method {method}"))),
        }
    }
}

pub struct VisionerEndpoint<T: IVisionerService>(pub Arc<T>);

#[async_trait]
impl<T: IVisionerService> RawRpcEndpoint for VisionerEndpoint<T> {
    fn service_name(&self) -> &'static str { "visioner.v1.VisionerService" }
    fn methods(&self) -> &'static [&'static str] { &["Capture"] }

    async fn dispatch(
        &self,
        method: &str,
        payload: &[u8],
        progress: ProgressSender,
    ) -> Result<Vec<u8>, RpcError> {
        match method {
            "Capture" => {
                let req = VisionCaptureRequest::decode(payload)
                    .map_err(|e| RpcError::invalid_arg(e.to_string()))?;
                let res = self.0.capture(req, progress).await?;
                Ok(res.encode_to_vec())
            }
            _ => Err(RpcError::not_found(format!("unknown method {method}"))),
        }
    }
}
