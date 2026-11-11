use super::*;

#[test]
fn sse_chunks_split_anywhere_still_give_the_answer() {
    let mut sse = SseText::default();
    let first = sse.push(b"data: {\"model\":\"m1\",\"choices\":[{\"delta\":{\"content\":\"Hel");
    assert!(first.is_empty());
    let rest = sse.push(b"lo\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\ndata: [DONE]\n\n");
    assert_eq!(rest, vec!["Hello", " world"]);
    assert_eq!(sse.text, "Hello world");
    assert_eq!(sse.model.as_deref(), Some("m1"));
}
