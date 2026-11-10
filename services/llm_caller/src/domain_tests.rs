//! Unit tests for `domain.rs`: the base-class steps, with a fake driver.

use super::*;
use crate::config::{DriverKind, LlmSettings};

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// Sends the given deltas, then returns `text` (or `error`).
struct FakeDriver {
    deltas: Vec<Delta>,
    result: Result<LlmReply, LlmError>,
}

impl LlmDriver for FakeDriver {
    fn call(&self, _request: LlmRequest, deltas: mpsc::UnboundedSender<Delta>) -> DriverFuture {
        for delta in &self.deltas {
            let _ = deltas.send(delta.clone());
        }
        let result = self.result.clone();
        Box::pin(async move { result })
    }
}

fn caller(text: &str) -> LlmCaller {
    let reply = LlmReply { text: text.to_string(), ..Default::default() };
    let driver = FakeDriver { deltas: vec![Delta::Text(text.to_string())], result: Ok(reply) };
    let config = LlmSettings::new("llm-test", DriverKind::OpenAi, "m");
    LlmCaller::new(Arc::new(config), Arc::new(driver))
}

fn user(text: &str) -> Vec<ChatMessage> {
    vec![ChatMessage::new(Role::User, text.to_string())]
}

async fn run(caller: &LlmCaller) -> (Result<LlmAnswer, LlmError>, Vec<Progress>) {
    let updates = std::sync::Mutex::new(Vec::new());
    let report = |update: Progress| updates.lock().unwrap().push(update);
    let result = caller.call(user("hi"), None, &report).await;
    (result, updates.into_inner().unwrap())
}

// ---------------------------------------------------------------------------------------------
// LlmCaller
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn json_answer_is_returned_compact() {
    let (result, _) = run(&caller("```json\n{\"a\": 1}\n```")).await;
    assert_eq!(result.unwrap().json, r#"{"a":1}"#);
}

#[tokio::test]
async fn first_progress_is_started() {
    let (_, updates) = run(&caller("{}")).await;
    assert_eq!(updates[0].stage, Stage::Started);
}

#[tokio::test]
async fn non_json_answer_is_invalid_json() {
    let (result, _) = run(&caller("sorry, no")).await;
    assert_eq!(result.unwrap_err(), LlmError::InvalidJson { excerpt: "sorry, no".into() });
}

#[tokio::test]
async fn driver_error_is_returned() {
    let driver = FakeDriver { deltas: vec![], result: Err(LlmError::RateLimited("slow down".into())) };
    let caller = LlmCaller::new(Arc::new(LlmSettings::new("x", DriverKind::OpenAi, "m")), Arc::new(driver));
    let (result, _) = run(&caller).await;
    assert_eq!(result.unwrap_err(), LlmError::RateLimited("slow down".into()));
}

// ---------------------------------------------------------------------------------------------
// prepare / parse_role
// ---------------------------------------------------------------------------------------------

#[test]
fn json_instruction_comes_first() {
    let request = prepare(user("hi"), Some("{}".into())).unwrap();
    assert_eq!(request.messages[0].role, Role::System);
    assert!(request.messages[0].content.contains("JSON"));
    assert_eq!(request.messages[1].content, "hi");
}

#[test]
fn schema_must_be_json() {
    assert!(matches!(prepare(user("hi"), Some("{not json".into())), Err(LlmError::InvalidRequest(_))));
}

#[test]
fn chat_without_user_text_is_invalid() {
    let only_system = vec![ChatMessage::new(Role::System, "be nice")];
    assert!(matches!(prepare(only_system, None), Err(LlmError::InvalidRequest(_))));
    assert!(matches!(prepare(user("  "), None), Err(LlmError::InvalidRequest(_))));
}

fn with_image(role: Role, mime_type: &str, bytes: usize) -> Vec<ChatMessage> {
    let mut message = ChatMessage::new(role, "look");
    message.images.push(Image { data: vec![0; bytes], mime_type: mime_type.into() });
    vec![ChatMessage::new(Role::User, "hi"), message]
}

#[test]
fn images_are_checked() {
    assert!(prepare(with_image(Role::User, "image/png", 10), None).is_ok());
    let bad = [
        with_image(Role::Assistant, "image/png", 10), // only on user messages
        with_image(Role::User, "image/tiff", 10),     // unknown type
        with_image(Role::User, "image/png", 0),       // empty
        with_image(Role::User, "image/png", MAX_IMAGE_BYTES + 1), // too big
    ];
    for messages in bad {
        assert!(matches!(prepare(messages, None), Err(LlmError::InvalidRequest(_))));
    }
}

#[test]
fn with_images_the_instruction_allows_opening_them() {
    let request = prepare(with_image(Role::User, "image/png", 10), None).unwrap();
    assert!(request.messages[0].content.contains("open the attached image files"));
}

#[test]
fn roles_are_parsed_case_insensitive() {
    assert_eq!(parse_role("Assistant").unwrap(), Role::Assistant);
    assert!(parse_role("tool").is_err());
}

// ---------------------------------------------------------------------------------------------
// ProgressTracker
// ---------------------------------------------------------------------------------------------

#[test]
fn text_is_accumulated_and_throttled() {
    let t0 = Instant::now();
    let mut tracker = ProgressTracker::new(t0);
    let first = tracker.add(Delta::Text("Hel".into()), t0).unwrap(); // stage change → sent
    assert_eq!((first.stage, first.partial_text.as_str()), (Stage::Writing, "Hel"));
    assert_eq!(tracker.add(Delta::Text("lo".into()), t0 + Duration::from_millis(10)), None);
    let later = tracker.add(Delta::Text("!".into()), t0 + Duration::from_millis(300)).unwrap();
    assert_eq!(later.partial_text, "Hello!");
    assert_eq!(later.elapsed, Duration::from_millis(300));
}

#[test]
fn stage_change_is_sent_at_once() {
    let t0 = Instant::now();
    let mut tracker = ProgressTracker::new(t0);
    assert_eq!(tracker.add(Delta::Thinking, t0).unwrap().stage, Stage::Thinking);
    assert_eq!(tracker.add(Delta::Text("x".into()), t0).unwrap().stage, Stage::Writing);
}
