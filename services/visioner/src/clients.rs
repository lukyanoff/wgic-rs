//! The domain's `Screen` and `VisionLlm` over the generated RPC clients (≈ C# adapters around typed
//! HTTP clients). The clients come from the factories in `libs/clients`, by instance id; every call
//! carries its timeout as a deadline, so the callee stops too when it passes.

use std::sync::Arc;
use std::time::Duration;

use clients::{LlmCallerClients, ScreenSensorClients};
use connectrpc::ConnectError;
use contracts::proto::llm_caller::v1::{
    CallRequest, CallResult, ChatMessage, Image, LlmCallerServiceClient, call_response::Event,
};
use contracts::proto::screen_sensor::v1::{CaptureRequest, ScreenSensorServiceClient};
use futures::future::BoxFuture;
use kernel::Channel;

use crate::domain::{
    ANSWER_SCHEMA, AskFuture, CallFailure, LlmReply, ProgressSink, Screen, ScreenInfo, Screenshot, VisionLlm,
};

// ---------------------------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------------------------

/// `Screen` = `ScreenSensorService/Capture` on one instance.
pub struct ScreenClient {
    client: ScreenSensorServiceClient<Channel>,
}

impl ScreenClient {
    pub fn new(factory: &ScreenSensorClients, instance: &str, timeout: Duration) -> Self {
        Self { client: factory.get(instance, timeout) }
    }
}

impl Screen for ScreenClient {
    fn capture(&self) -> BoxFuture<'static, Result<Screenshot, CallFailure>> {
        let client = self.client.clone();
        Box::pin(async move {
            let response = client.capture(CaptureRequest::default()).await.map_err(failure)?.into_owned();
            let meta = &response.metadata;
            let info = ScreenInfo {
                sensor_id: meta.sensor_id.clone(),
                width: meta.width,
                height: meta.height,
                mime_type: meta.mime_type.clone(),
                bytes: response.image.len() as u64,
                capture_ms: meta.capture_duration_ms,
            };
            Ok(Screenshot { image: response.image.to_vec(), info })
        })
    }
}

// ---------------------------------------------------------------------------------------------
// LLM
// ---------------------------------------------------------------------------------------------

/// `VisionLlm` = `LlmCallerService/Call` on one instance; its progress is passed on as this source's.
pub struct LlmClient {
    id: String,
    client: LlmCallerServiceClient<Channel>,
}

impl LlmClient {
    pub fn new(factory: &LlmCallerClients, instance: &str, timeout: Duration) -> Self {
        Self { id: instance.to_string(), client: factory.get(instance, timeout) }
    }
}

impl VisionLlm for LlmClient {
    fn id(&self) -> &str {
        &self.id
    }

    fn ask(&self, question: String, screenshot: Arc<Screenshot>, progress: ProgressSink) -> AskFuture {
        let request = call_request(&question, &screenshot);
        Box::pin(call_llm(self.client.clone(), request, progress))
    }
}

/// Run one `Call`: pass each progress message on, return the result.
async fn call_llm(
    client: LlmCallerServiceClient<Channel>,
    request: CallRequest,
    progress: ProgressSink,
) -> Result<LlmReply, CallFailure> {
    let mut stream = client.call(request).await.map_err(failure)?;
    while let Some(message) = stream.message().await.map_err(failure)? {
        match message.to_owned_message().event {
            Some(Event::Progress(p)) => progress.report(&p.stage, &p.partial_text),
            Some(Event::Result(result)) => return Ok(reply(*result)),
            None => {}
        }
    }
    Err(CallFailure { code: "internal".into(), message: "the stream ended without a result".into() })
}

/// The question and the screenshot as one user message, asking for `ANSWER_SCHEMA` JSON.
fn call_request(question: &str, screenshot: &Screenshot) -> CallRequest {
    let image =
        Image { data: screenshot.image.clone(), mime_type: screenshot.info.mime_type.clone(), ..Default::default() };
    let message =
        ChatMessage { role: "user".into(), content: question.into(), images: vec![image], ..Default::default() };
    CallRequest { messages: vec![message], json_schema: ANSWER_SCHEMA.into(), ..Default::default() }
}

fn reply(result: CallResult) -> LlmReply {
    LlmReply { json: result.json, text: result.text, model: result.metadata.model.clone() }
}

fn failure(error: ConnectError) -> CallFailure {
    CallFailure { code: error.code.as_str().to_string(), message: error.message.unwrap_or_default() }
}

#[cfg(test)]
#[path = "clients_tests.rs"]
mod tests;
