//! Integration: the real visioner module calls fake `screen_sensor` and `llm_caller` services by instance
//! id, in-process, through the `libs/clients` factories (the same path as production, without NATS).

use std::sync::Arc;
use std::time::Duration;

use clients::{LlmCallerClients, ScreenSensorClients};
use connectrpc::{
    ConnectError, ErrorCode, RequestContext, Response, Router, ServiceRequest, ServiceResult, ServiceStream,
};
use contracts::proto::llm_caller::v1::{
    CallMetadata, CallProgress, CallRequest, CallResponse, CallResult, LlmCallerService, call_response,
};
use contracts::proto::screen_sensor::v1::{
    CaptureRequest, CaptureResponse, ListDevicesRequest, ListDevicesResponse, ScreenSensorService,
};
use contracts::proto::sensor::v1::ImageMetadata;
use contracts::proto::visioner::v1::{CheckRequest, CheckResponse, VisionerServiceClient, check_response::Event};
use kernel::testing::{MemoryLogs, MemoryMetrics};
use kernel::{Deps, Module};
use visioner::clients::{LlmClient, ScreenClient};
use visioner::config::VisionerSettings;
use visioner::domain::VisionLlm;

// ---------------------------------------------------------------------------------------------
// Fake services
// ---------------------------------------------------------------------------------------------

struct FakeScreen;

#[allow(refining_impl_trait)]
impl ScreenSensorService for FakeScreen {
    async fn capture(
        &self,
        _ctx: RequestContext,
        _req: ServiceRequest<'_, CaptureRequest>,
    ) -> ServiceResult<CaptureResponse> {
        let metadata = ImageMetadata {
            sensor_id: "screen-1".into(),
            width: 4,
            height: 2,
            mime_type: "image/png".into(),
            ..Default::default()
        };
        Response::ok(CaptureResponse {
            image: vec![0x89, b'P', b'N', b'G'],
            metadata: metadata.into(),
            ..Default::default()
        })
    }

    async fn list_devices(
        &self,
        _ctx: RequestContext,
        _req: ServiceRequest<'_, ListDevicesRequest>,
    ) -> ServiceResult<ListDevicesResponse> {
        Response::ok(ListDevicesResponse::default())
    }
}

/// How a fake model behaves.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Streams "writing" twice, then answers `{"summary":"<name>"}`; checks it got an image.
    Answers(&'static str),
    Fails,
    /// Never finishes.
    Hangs,
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
            Behaviour::Fails => return Err(ConnectError::new(ErrorCode::Internal, "model crashed")),
            Behaviour::Hangs => return Response::ok(Box::pin(futures::stream::pending())),
            Behaviour::Answers(name) if has_image => name,
            Behaviour::Answers(_) => return Err(ConnectError::new(ErrorCode::InvalidArgument, "no image")),
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

/// A `Module` around one fake service.
struct Fake<S>(Arc<S>);

impl Module for Fake<FakeScreen> {
    fn name(&self) -> &'static str {
        "screen_sensor"
    }
    fn register(&self, router: Router) -> Router {
        router.add_service(self.0.clone())
    }
}

impl Module for Fake<FakeLlm> {
    fn name(&self) -> &'static str {
        "llm_caller"
    }
    fn register(&self, router: Router) -> Router {
        router.add_service(self.0.clone())
    }
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
fn setup(a: Behaviour, b: Behaviour) -> Setup {
    let screen: Arc<dyn Module> = Arc::new(Fake(Arc::new(FakeScreen)));
    let llm_a: Arc<dyn Module> = Arc::new(Fake(Arc::new(FakeLlm(a))));
    let llm_b: Arc<dyn Module> = Arc::new(Fake(Arc::new(FakeLlm(b))));
    let (logs, metrics) = (MemoryLogs::new(), MemoryMetrics::new());
    let deps =
        Deps::in_process_instances(vec![("screen-1", vec![screen]), ("llm-a", vec![llm_a]), ("llm-b", vec![llm_b])])
            .with_logs(logs.clone())
            .with_metrics(metrics.clone());
    let settings = settings();
    let screen = ScreenClient::new(&ScreenSensorClients::new(&deps), &settings.screen, settings.screen_timeout);
    let factory = LlmCallerClients::new(&deps);
    let llms: Vec<Arc<dyn VisionLlm>> = settings
        .llms
        .iter()
        .map(|id| Arc::new(LlmClient::new(&factory, id, settings.llm_timeout)) as Arc<dyn VisionLlm>)
        .collect();
    let module = visioner::module_with(&deps, &settings, Arc::new(screen), llms);
    let caller = Deps::in_process(&[module]);
    Setup { client: VisionerServiceClient::new(caller.channel(), caller.client_config()), logs, metrics }
}

fn settings() -> VisionerSettings {
    VisionerSettings {
        id: "vision-test".into(),
        screen: "screen-1".into(),
        llms: vec!["llm-a".into(), "llm-b".into()],
        screen_timeout: Duration::from_secs(2),
        llm_timeout: Duration::from_secs(1),
    }
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

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn both_models_answer_with_routed_progress() {
    let setup = setup(Behaviour::Answers("a"), Behaviour::Answers("b"));
    let all = check(&setup).await.unwrap();
    let Some(Event::Result(result)) = &all.last().unwrap().event else { panic!("the result comes last") };
    assert_eq!((result.screen.sensor_id.as_str(), result.screen.bytes), ("screen-1", 4));
    let jsons: Vec<&str> = result.answers.iter().map(|a| a.json.as_str()).collect();
    assert_eq!(jsons, [r#"{"summary":"a"}"#, r#"{"summary":"b"}"#]);
    assert_eq!(result.answers[0].model, "fake-model");
    let stages = stages(&all);
    assert_eq!(&stages[..2], ["screen/capturing", "screen/captured"]);
    for expected in ["llm-a/writing", "llm-b/writing", "llm-a/done", "llm-b/done"] {
        assert!(stages.contains(&expected.to_string()), "{stages:?}");
    }
    assert!(setup.logs.find("check done").is_some());
    assert_eq!(setup.metrics.sum("visioner.checks"), 1.0);
}

#[tokio::test]
async fn failed_and_hanging_models_leave_one_answer() {
    let setup = setup(Behaviour::Hangs, Behaviour::Answers("b"));
    let all = check(&setup).await.unwrap();
    let Some(Event::Result(result)) = &all.last().unwrap().event else { panic!("the result comes last") };
    let a = &result.answers[0];
    assert!(!a.ok && a.error_code == "deadline_exceeded", "{a:?}");
    assert!(result.answers[1].ok);
    assert!(stages(&all).contains(&"llm-a/timeout".to_string()));
    assert_eq!(setup.metrics.sum("visioner.llm_failures"), 1.0);
}

#[tokio::test]
async fn all_models_failing_is_unavailable() {
    let setup = setup(Behaviour::Fails, Behaviour::Fails);
    let error = check(&setup).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(error.message.unwrap_or_default().contains("llm-a: model crashed (internal)"));
    assert!(setup.logs.find("check failed").is_some());
}
