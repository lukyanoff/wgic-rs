//! Integration: the real visioner module calls fake `sensor` and `llm_caller` instances by id, in-process
//! (the same clients as production, without NATS).

use std::time::Duration;

use connectrpc::{ConnectError, ErrorCode, RequestContext, Response, ServiceRequest, ServiceResult, ServiceStream};
use contracts::proto::llm_caller::v1::{
    CallMetadata, CallProgress, CallRequest, CallResponse, CallResult, LlmCallerService, call_response,
};
use contracts::proto::sensor::v1::{
    CaptureRequest, CaptureResponse, ImageMetadata, ListDevicesRequest, ListDevicesResponse, SensorService,
};
use contracts::proto::visioner::v1::{CheckRequest, CheckResponse, VisionerServiceClient, check_response::Event};
use kernel::Deps;
use kernel::testing::{MemoryLogs, MemoryMetrics};
use visioner::VisionerSettings;

// ---------------------------------------------------------------------------------------------
// Fake services
// ---------------------------------------------------------------------------------------------

/// How a fake instance behaves.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Screen: a 4-byte PNG. Model: streams "writing" twice, then answers `{"summary":"<name>"}` (needs an image).
    Answers(&'static str),
    Fails,
    /// Never finishes.
    Hangs,
}

struct FakeScreen(Behaviour);

#[allow(refining_impl_trait)]
impl SensorService for FakeScreen {
    async fn capture(
        &self,
        _ctx: RequestContext,
        _req: ServiceRequest<'_, CaptureRequest>,
    ) -> ServiceResult<CaptureResponse> {
        match self.0 {
            Behaviour::Hangs => futures::future::pending().await,
            Behaviour::Fails => Err(ConnectError::not_found("no display")),
            Behaviour::Answers(id) => {
                let metadata =
                    ImageMetadata { sensor_id: id.into(), mime_type: "image/png".into(), ..Default::default() };
                let image = vec![0x89, b'P', b'N', b'G'];
                Response::ok(CaptureResponse { image, metadata: metadata.into(), ..Default::default() })
            }
        }
    }

    async fn list_devices(
        &self,
        _ctx: RequestContext,
        _req: ServiceRequest<'_, ListDevicesRequest>,
    ) -> ServiceResult<ListDevicesResponse> {
        Response::ok(ListDevicesResponse::default())
    }
}

struct FakeLlm(Behaviour);

#[allow(refining_impl_trait)]
impl LlmCallerService for FakeLlm {
    async fn call(
        &self,
        _ctx: RequestContext,
        req: ServiceRequest<'_, CallRequest>,
    ) -> ServiceResult<ServiceStream<CallResponse>> {
        let has_image = req.to_owned_message().messages.iter().any(|m| !m.images.is_empty());
        let name = match self.0 {
            Behaviour::Fails => return Err(ConnectError::internal("model crashed")),
            Behaviour::Hangs => return Response::ok(Box::pin(futures::stream::pending())),
            Behaviour::Answers(name) if has_image => name,
            Behaviour::Answers(_) => return Err(ConnectError::invalid_argument("no image")),
        };
        let json = format!("{{\"summary\":\"{name}\"}}");
        let messages = vec![progress("writing", "{\"sum"), progress("writing", &json), result(&json)];
        Response::ok(Box::pin(futures::stream::iter(messages.into_iter().map(Ok))))
    }
}

fn progress(stage: &str, partial_text: &str) -> CallResponse {
    let message = CallProgress { stage: stage.into(), partial_text: partial_text.into(), ..Default::default() };
    CallResponse { event: Some(call_response::Event::from(message)), ..Default::default() }
}

fn result(json: &str) -> CallResponse {
    let metadata = CallMetadata { model: "fake-model".into(), ..Default::default() };
    let message = CallResult { json: json.into(), text: json.into(), metadata: metadata.into(), ..Default::default() };
    CallResponse { event: Some(call_response::Event::from(message)), ..Default::default() }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

struct Setup {
    client: VisionerServiceClient<kernel::Channel>,
    logs: MemoryLogs,
    metrics: MemoryMetrics,
}

/// Instances `screen-1`, `llm-a`, `llm-b` (behaviours as given), and a visioner configured with them.
fn setup(screen: Behaviour, a: Behaviour, b: Behaviour) -> Setup {
    let (logs, metrics) = (MemoryLogs::new(), MemoryMetrics::new());
    let deps = Deps::in_process_instances(vec![
        ("screen-1", vec![kernel::module("sensor", FakeScreen(screen))]),
        ("llm-a", vec![kernel::module("llm_caller", FakeLlm(a))]),
        ("llm-b", vec![kernel::module("llm_caller", FakeLlm(b))]),
    ])
    .with_logs(logs.clone())
    .with_metrics(metrics.clone());
    let settings = VisionerSettings {
        id: "vision-test".into(),
        screen: "screen-1".into(),
        llms: vec!["llm-a".into(), "llm-b".into()],
        screen_timeout: Duration::from_millis(300),
        llm_timeout: Duration::from_millis(300),
    };
    let caller = Deps::in_process(&[visioner::module_with(&deps, &settings)]);
    Setup { client: VisionerServiceClient::new(caller.channel(), caller.client_config()), logs, metrics }
}

async fn check(setup: &Setup) -> Result<Vec<CheckResponse>, ConnectError> {
    let mut stream = setup.client.check(CheckRequest::default()).await?;
    let mut all = Vec::new();
    while let Some(message) = stream.message().await? {
        all.push(message.to_owned_message());
    }
    Ok(all)
}

/// Every progress as "source/stage".
fn stages(all: &[CheckResponse]) -> Vec<String> {
    let mut out = Vec::new();
    for message in all {
        if let Some(Event::Progress(p)) = &message.event {
            out.push(format!("{}/{}", p.source, p.stage));
        }
    }
    out
}

use Behaviour::{Answers, Fails, Hangs};

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn both_models_answer_with_routed_progress() {
    let setup = setup(Answers("screen-1"), Answers("a"), Answers("b"));
    let all = check(&setup).await.unwrap();
    let Some(Event::Result(result)) = &all.last().unwrap().event else { panic!("the result comes last") };
    assert_eq!(result.screen.sensor_id, "screen-1");
    let jsons: Vec<&str> = result.answers.iter().map(|a| a.json.as_str()).collect();
    assert_eq!(jsons, [r#"{"summary":"a"}"#, r#"{"summary":"b"}"#]);
    assert_eq!(result.answers[0].model, "fake-model");
    let stages = stages(&all);
    assert_eq!(&stages[..2], ["screen/capturing", "screen/captured"]);
    for expected in ["llm-a/writing", "llm-b/writing", "llm-a/done", "llm-b/done"] {
        assert!(stages.contains(&expected.to_string()), "{stages:?}");
    }
}

#[tokio::test]
async fn a_hanging_model_times_out_and_the_other_answers() {
    let setup = setup(Answers("screen-1"), Hangs, Answers("b"));
    let all = check(&setup).await.unwrap();
    let Some(Event::Result(result)) = &all.last().unwrap().event else { panic!("the result comes last") };
    let a = &result.answers[0];
    assert!(!a.ok && a.error_code == "deadline_exceeded", "{a:?}");
    assert!(result.answers[1].ok);
    assert!(stages(&all).contains(&"llm-a/timeout".to_string()));
    assert!(setup.logs.find("llm failed").is_some());
    assert_eq!(setup.metrics.sum("visioner.llm_failures"), 1.0);
}

#[tokio::test]
async fn all_models_failing_is_unavailable() {
    let error = check(&setup(Answers("screen-1"), Fails, Fails)).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(error.message.unwrap_or_default().contains("llm-a: model crashed (internal)"));
}

#[tokio::test]
async fn all_models_timing_out_is_deadline_exceeded() {
    let error = check(&setup(Answers("screen-1"), Hangs, Hangs)).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::DeadlineExceeded);
}

#[tokio::test]
async fn no_screenshot_fails_the_check() {
    assert_eq!(check(&setup(Fails, Answers("a"), Answers("b"))).await.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(check(&setup(Hangs, Answers("a"), Answers("b"))).await.unwrap_err().code, ErrorCode::DeadlineExceeded);
}
