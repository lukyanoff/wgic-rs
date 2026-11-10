//! Unit tests for `domain.rs` with fake screens and models.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

// ---------------------------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------------------------

/// Answers after `delay`, or fails with `failure`.
struct FakeScreen {
    delay: Duration,
    failure: Option<CallFailure>,
}

impl Screen for FakeScreen {
    fn capture(&self) -> BoxFuture<'static, Result<Screenshot, CallFailure>> {
        let (delay, failure) = (self.delay, self.failure.clone());
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            match failure {
                Some(failure) => Err(failure),
                None => Ok(Screenshot {
                    image: vec![1, 2],
                    info: ScreenInfo { sensor_id: "s1".into(), ..Default::default() },
                }),
            }
        })
    }
}

fn screen() -> Arc<dyn Screen> {
    Arc::new(FakeScreen { delay: Duration::ZERO, failure: None })
}

/// Reports "writing", then answers `{"summary":"<id>"}` after `delay`, or fails. Sets `dropped` when its
/// call is dropped before it finished (= cancelled).
struct FakeLlm {
    id: &'static str,
    delay: Duration,
    failure: Option<CallFailure>,
    dropped: Arc<AtomicBool>,
}

fn llm(id: &'static str, delay_ms: u64, failure: Option<&str>) -> Arc<FakeLlm> {
    let failure = failure.map(|code| CallFailure { code: code.into(), message: format!("{id} broke") });
    Arc::new(FakeLlm { id, delay: Duration::from_millis(delay_ms), failure, dropped: Arc::default() })
}

/// Sets the flag when dropped, unless disarmed (≈ a `finally` that notices cancellation).
struct DropFlag(Arc<AtomicBool>, bool);

impl Drop for DropFlag {
    fn drop(&mut self) {
        if self.1 {
            self.0.store(true, Ordering::SeqCst);
        }
    }
}

impl VisionLlm for FakeLlm {
    fn id(&self) -> &str {
        self.id
    }

    fn ask(&self, _question: String, _screenshot: Arc<Screenshot>, progress: ProgressSink) -> AskFuture {
        let (id, delay, failure) = (self.id, self.delay, self.failure.clone());
        let flag = DropFlag(self.dropped.clone(), true);
        Box::pin(async move {
            let mut flag = flag; // move the whole guard into the call
            progress.report("writing", "{\"summ");
            tokio::time::sleep(delay).await;
            flag.1 = false; // finished: not a cancel
            match failure {
                Some(failure) => Err(failure),
                None => Ok(LlmReply { json: format!("{{\"summary\":\"{id}\"}}"), ..Default::default() }),
            }
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn visioner(screen: Arc<dyn Screen>, llms: &[Arc<FakeLlm>]) -> Visioner {
    let llms: Vec<Arc<dyn VisionLlm>> = llms.iter().map(|l| l.clone() as Arc<dyn VisionLlm>).collect();
    Visioner::new(screen, llms, (Duration::from_millis(200), Duration::from_millis(300)))
}

/// Run a check; return its result and every progress update as "source/stage".
async fn run(visioner: &Visioner) -> (Result<CheckOutcome, VisionError>, Vec<String>) {
    let seen = Mutex::new(Vec::new());
    let report = |p: Progress| seen.lock().expect("lock").push(format!("{}/{}", p.source, p.stage));
    let result = visioner.check("", &report).await;
    (result, seen.into_inner().expect("lock"))
}

fn ok_ids(outcome: &CheckOutcome) -> Vec<bool> {
    outcome.answers.iter().map(|a| a.outcome.is_ok()).collect()
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_model_answers() {
    let (result, progress) = run(&visioner(screen(), &[llm("a", 10, None), llm("b", 20, None)])).await;
    let outcome = result.unwrap();
    assert_eq!(ok_ids(&outcome), [true, true]);
    assert_eq!(outcome.answers[1].outcome.as_ref().unwrap().json, r#"{"summary":"b"}"#);
    assert_eq!(&progress[..2], ["screen/capturing", "screen/captured"]);
    for expected in ["a/writing", "b/writing", "a/done", "b/done"] {
        assert!(progress.contains(&expected.to_string()), "{progress:?}");
    }
}

#[tokio::test]
async fn one_failed_model_does_not_fail_the_check() {
    let (result, progress) = run(&visioner(screen(), &[llm("a", 10, Some("unavailable")), llm("b", 10, None)])).await;
    let outcome = result.unwrap();
    assert_eq!(ok_ids(&outcome), [false, true]);
    assert_eq!(outcome.answers[0].outcome.as_ref().unwrap_err().code, "unavailable");
    assert!(progress.contains(&"a/failed".to_string()));
}

#[tokio::test]
async fn slow_model_times_out_and_is_cancelled() {
    let slow = llm("slow", 5_000, None);
    let started = Instant::now();
    let (result, progress) = run(&visioner(screen(), &[slow.clone(), llm("fast", 10, None)])).await;
    assert!(started.elapsed() < Duration::from_secs(2), "the check waited for the slow model");
    let outcome = result.unwrap();
    assert_eq!(outcome.answers[0].outcome.as_ref().unwrap_err().code, DEADLINE_EXCEEDED);
    assert!(progress.contains(&"slow/timeout".to_string()));
    assert!(slow.dropped.load(Ordering::SeqCst), "the slow call was not cancelled");
}

#[tokio::test]
async fn all_models_failing_fails_the_check() {
    let (result, _) = run(&visioner(screen(), &[llm("a", 10, Some("internal")), llm("b", 5_000, None)])).await;
    let Err(VisionError::AllLlmsFailed { summary, all_timed_out }) = result else { panic!("expected AllLlmsFailed") };
    assert!(summary.contains("a: a broke (internal)") && summary.contains("b: "), "{summary}");
    assert!(!all_timed_out);
}

#[tokio::test]
async fn screen_failure_or_timeout_fails_the_check() {
    let failure = CallFailure { code: "unavailable".into(), message: "no display".into() };
    let broken = Arc::new(FakeScreen { delay: Duration::ZERO, failure: Some(failure.clone()) });
    let (result, _) = run(&visioner(broken, &[llm("a", 10, None)])).await;
    assert_eq!(result.unwrap_err(), VisionError::Screen(failure));

    let slow = Arc::new(FakeScreen { delay: Duration::from_secs(5), failure: None });
    let (result, progress) = run(&visioner(slow, &[llm("a", 10, None)])).await;
    assert!(matches!(result, Err(VisionError::Screen(f)) if f.code == DEADLINE_EXCEEDED));
    assert_eq!(progress, ["screen/capturing"], "no model is asked without a screenshot");
}
