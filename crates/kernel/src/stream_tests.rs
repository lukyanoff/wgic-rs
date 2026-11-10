//! Unit tests for `stream.rs`.

use futures::StreamExt;
use futures::executor::block_on;

use super::*;

#[test]
fn every_send_becomes_a_message_in_order() {
    let stream = stream_from(|tx| async move {
        tx.send(1);
        tx.send(2);
    });
    let messages: Vec<i32> = block_on(stream.map(|item| item.unwrap()).collect());
    assert_eq!(messages, [1, 2]);
}

#[test]
fn fail_ends_the_stream_with_the_error() {
    let mut stream = stream_from(|tx: StreamTx<i32>| async move {
        tx.fail(ConnectError::unavailable("down"));
    });
    let error = block_on(stream.next()).unwrap().unwrap_err();
    assert_eq!(error.code, connectrpc::ErrorCode::Unavailable);
    assert!(block_on(stream.next()).is_none());
}

#[test]
fn clones_of_tx_all_feed_the_same_stream() {
    let stream = stream_from(|tx| async move {
        let other = tx.clone();
        tx.send("a");
        other.send("b");
    });
    let messages: Vec<&str> = block_on(stream.map(|item| item.unwrap()).collect());
    assert_eq!(messages, ["a", "b"]);
}
