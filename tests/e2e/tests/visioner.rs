//! E2E: VisionerService orchestrating real hosts, all over NATS.
//!
//! Four `host` processes, each its own NATS instance id:
//! - a `sensor` (captures this machine's screen as JPEG)
//! - `llm ok`: llm_caller with the `openai` driver against a fake server that answers
//! - `llm hang`: the same against a fake server that never answers
//! - the visioner, configured with those three ids and a 3 s model timeout
//!
//! Expected: progress from the screen and both models, then a result where `ok` answered and `hang` timed
//! out, and the hanging provider call was cancelled. Without a display the screen answers `unavailable`
//! (like the sensor e2e), and so does the check.

use std::time::Duration;

use connectrpc::{ConnectError, ErrorCode};
use contracts::proto::visioner::v1::{CheckRequest, CheckResponse, VisionerServiceClient, check_response::Event};
use e2e::Host;
use e2e::fake_openai::{Seen, fake_openai};

async fn llm_host(hang: bool, seen: Seen) -> Host {
    let base_url = fake_openai(hang, seen).await;
    let env = [
        ("LLM_CALLER_DRIVER", "openai"),
        ("LLM_CALLER_MODEL", "fake-model"),
        ("LLM_CALLER_EFFORT", "low"),
        ("LLM_CALLER_BASE_URL", base_url.as_str()),
        ("OPENAI_API_KEY", "test-key"),
    ];
    Host::start_with_env("llm_caller", &env).await
}

async fn check(visioner: &Host) -> Result<Vec<CheckResponse>, ConnectError> {
    let deps = visioner.nats_deps().await;
    let client = VisionerServiceClient::new(deps.channel(), deps.client_config());
    let mut stream = client.check(CheckRequest::default()).await?;
    let mut all = Vec::new();
    while let Some(message) = stream.message().await? {
        all.push(message.to_owned_message());
    }
    Ok(all)
}

fn sources(all: &[CheckResponse]) -> Vec<String> {
    let mut out = Vec::new();
    for message in all {
        if let Some(Event::Progress(p)) = &message.event {
            out.push(format!("{}/{}", p.source, p.stage));
        }
    }
    out
}

#[tokio::test]
async fn one_answer_one_timeout_and_the_slow_call_is_cancelled() {
    let screen = Host::start_with_env("sensor", &[("SENSOR_FORMAT", "jpeg")]).await;
    let ok = llm_host(false, Seen::default()).await;
    let hang_seen = Seen::default();
    let hang = llm_host(true, hang_seen.clone()).await;
    let llms = format!("{},{}", ok.nats_prefix(), hang.nats_prefix());
    let env = [
        ("VISIONER_ID", "vision-e2e"),
        ("VISIONER_SCREEN", screen.nats_prefix()),
        ("VISIONER_LLMS", llms.as_str()),
        ("VISIONER_LLM_TIMEOUT_SECS", "3"),
    ];
    let visioner = Host::start_with_env("visioner", &env).await;

    let all = match check(&visioner).await {
        Ok(all) => all,
        Err(error) if error.code == ErrorCode::Unavailable => return, // no display on this machine
        Err(error) => panic!("unexpected error: {error:?}"),
    };
    let Some(Event::Result(result)) = &all.last().expect("messages").event else { panic!("the result comes last") };
    assert!(result.screen.width > 0 && result.screen.mime_type == "image/jpeg", "{:?}", result.screen);
    let (answer_ok, answer_hang) = (&result.answers[0], &result.answers[1]);
    assert!(answer_ok.ok && answer_ok.json == r#"{"city":"Paris","country":"FR"}"#, "{answer_ok:?}");
    assert!(!answer_hang.ok && answer_hang.error_code == "deadline_exceeded", "{answer_hang:?}");

    let sources = sources(&all);
    for expected in [
        "screen/captured".to_string(),
        format!("{}/writing", ok.nats_prefix()),
        format!("{}/timeout", hang.nats_prefix()),
    ] {
        assert!(sources.contains(&expected), "missing {expected} in {sources:?}");
    }
    assert!(hang_seen.wait_for_hang_up(Duration::from_secs(5)).await, "the hanging provider call was not cancelled");
}

/// A unary request (`application/json`, as `nats req` sends) to the streaming `Check` is `unimplemented`
/// with a message that says how to call it, not a bare "method not found".
#[tokio::test]
async fn unary_request_over_nats_explains_that_check_is_streaming() {
    let env = [("VISIONER_SCREEN", "screen-x"), ("VISIONER_LLMS", "llm-x")]; // never called here
    let visioner = Host::start_with_env("visioner", &env).await;
    let nc = async_nats::connect(e2e::nats_url()).await.expect("connect NATS");
    let mut headers = async_nats::HeaderMap::new();
    headers.insert("Content-Type", "application/json");
    let subject = format!("{}.visioner.v1.VisionerService.Check", visioner.nats_prefix());
    let reply = nc.request_with_headers(subject, headers, "{}".into()).await.expect("a reply");
    let headers = reply.headers.expect("error headers");
    assert_eq!(headers.get("Connect-Code").map(|v| v.as_str()), Some("unimplemented"));
    let message = headers.get("Nats-Service-Error").map(|v| v.as_str().to_string()).unwrap_or_default();
    assert!(message.contains("is a streaming method"), "{message}");
}
