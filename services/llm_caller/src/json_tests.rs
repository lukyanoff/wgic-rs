//! Unit tests for `json.rs`.

use super::*;

#[test]
fn plain_json_is_compacted() {
    assert_eq!(extract_json("{ \"a\": 1 }\n").as_deref(), Some(r#"{"a":1}"#));
}

#[test]
fn fenced_json_is_found() {
    let text = "```json\n{\"items\": [1, 2]}\n```";
    assert_eq!(extract_json(text).as_deref(), Some(r#"{"items":[1,2]}"#));
}

#[test]
fn json_after_a_sentence_is_found() {
    let text = "The largest city is **Paris**.\n{\"city\":\"Paris\"}\n";
    assert_eq!(extract_json(text).as_deref(), Some(r#"{"city":"Paris"}"#));
}

#[test]
fn arrays_are_accepted() {
    assert_eq!(extract_json("[1, 2]").as_deref(), Some("[1,2]"));
}

#[test]
fn text_without_json_is_none() {
    assert_eq!(extract_json("no json here"), None);
    assert_eq!(extract_json("} backwards {"), None);
    assert_eq!(extract_json("{ broken"), None);
}

#[test]
fn instruction_contains_the_schema() {
    assert!(json_instruction(Some(r#"{"type":"object"}"#), false).contains(r#"{"type":"object"}"#));
    assert!(json_instruction(None, false).contains("JSON object or array"));
}

#[test]
fn excerpt_cuts_long_text() {
    assert_eq!(excerpt("abcdef", 3), "abc…");
    assert_eq!(excerpt("abc", 3), "abc");
}
