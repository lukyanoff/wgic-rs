//! Server-streaming RPCs over NATS.
//!
//! NATS is only a byte pipe here: a Connect streaming body is already a sequence of
//! envelopes (5-byte header + message, the last one is END_STREAM), so we forward it unchanged.
//! - server: every body chunk → one NATS message to the request's reply inbox, then an
//!   empty message with header `Nats-Stream-End: 1`. The first message carries the response headers.
//! - client: subscribe to an inbox, publish the request with that reply subject and turn the
//!   inbox messages back into a response body until the end marker.

use std::time::Duration;

use async_nats::{HeaderMap, Subject};
use bytes::Bytes;
use connectrpc::{ConnectError, ConnectRpcBody, ErrorCode};
use futures::StreamExt;
use http::{Response, response};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, StreamBody};

use super::wire::STREAM_END;

/// Response body of the NATS client transport (unary: one chunk; streaming: many).
pub type NatsBody = UnsyncBoxBody<Bytes, ConnectError>;

/// Without a Connect deadline, a stream whose server vanished fails after this much silence.
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

// ---------------------------------------------------------------------------------------------
// Server side
// ---------------------------------------------------------------------------------------------

/// Publish every chunk of a streaming response to `reply`, then the end marker.
pub(super) async fn publish_stream(
    nc: &async_nats::Client,
    reply: Subject,
    parts: &response::Parts,
    mut body: ConnectRpcBody,
) -> anyhow::Result<()> {
    let mut headers = response_headers(parts);
    while let Some(frame) = body.frame().await {
        let Ok(data) = frame?.into_data() else {
            continue; // trailers: Connect streaming has none, its END_STREAM envelope is data
        };
        nc.publish_with_headers(reply.clone(), std::mem::take(&mut headers), data).await?;
    }
    let mut end = std::mem::take(&mut headers); // still set if the body was empty
    end.insert(STREAM_END, "1");
    nc.publish_with_headers(reply, end, Bytes::new()).await?;
    Ok(())
}

fn response_headers(parts: &response::Parts) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in parts.headers.iter() {
        if let Ok(v) = value.to_str() {
            headers.append(name.as_str(), v);
        }
    }
    headers
}

// ---------------------------------------------------------------------------------------------
// Client side
// ---------------------------------------------------------------------------------------------

/// What the client needs to start one streaming call.
pub(super) struct StreamCall {
    pub subject: String,
    pub headers: HeaderMap,
    pub payload: Bytes,
    pub idle_timeout: Option<Duration>,
}

/// Send a streaming request and return the response once its first message arrived.
/// The first message decides between an error reply (NATS micro error headers) and a stream.
pub(super) async fn request_stream(
    nc: &async_nats::Client,
    call: StreamCall,
) -> Result<(HeaderMap, Response<NatsBody>), ConnectError> {
    let idle = call.idle_timeout.unwrap_or(DEFAULT_IDLE_TIMEOUT);
    let inbox = nc.new_inbox();
    let mut inbox_messages = nc.subscribe(inbox.clone()).await.map_err(|e| unavailable(&call.subject, e))?;
    nc.publish_with_reply_and_headers(call.subject.clone(), inbox, call.headers, call.payload)
        .await
        .map_err(|e| unavailable(&call.subject, e))?;
    let first = next_message(&mut inbox_messages, idle, &call.subject).await?;
    let headers = first.headers.clone().unwrap_or_default();
    let body = stream_body(first, inbox_messages, idle, call.subject);
    Ok((headers, Response::new(body)))
}

/// The first message's payload, then every next payload until `Nats-Stream-End`.
fn stream_body(first: async_nats::Message, inbox: async_nats::Subscriber, idle: Duration, subject: String) -> NatsBody {
    let state = Some((Some(first), inbox));
    let chunks = futures::stream::unfold(state, move |state| {
        let subject = subject.clone();
        async move {
            let (pending, mut inbox) = state?;
            let message = match pending {
                Some(message) => message,
                None => match next_message(&mut inbox, idle, &subject).await {
                    Ok(message) => message,
                    Err(error) => return Some((Err(error), None)),
                },
            };
            if is_stream_end(&message) {
                return Some((Ok(message.payload), None)); // payload is empty; ends the stream next poll
            }
            Some((Ok(message.payload), Some((None, inbox))))
        }
    });
    let frames = chunks.map(|chunk| chunk.map(http_body::Frame::data));
    BodyExt::boxed_unsync(StreamBody::new(frames))
}

async fn next_message(
    inbox: &mut async_nats::Subscriber,
    idle: Duration,
    subject: &str,
) -> Result<async_nats::Message, ConnectError> {
    let next = match tokio::time::timeout(idle, inbox.next()).await {
        Ok(next) => next,
        Err(_) => return Err(ConnectError::new(ErrorCode::DeadlineExceeded, format!("NATS stream {subject} idle"))),
    };
    let Some(message) = next else {
        return Err(ConnectError::new(ErrorCode::Unavailable, format!("NATS stream {subject} closed")));
    };
    if message.status == Some(async_nats::StatusCode::NO_RESPONDERS) {
        return Err(ConnectError::new(ErrorCode::Unavailable, format!("no NATS responders for {subject}")));
    }
    Ok(message)
}

fn is_stream_end(message: &async_nats::Message) -> bool {
    message.headers.as_ref().is_some_and(|h| h.get(STREAM_END).is_some())
}

fn unavailable(subject: &str, e: impl std::fmt::Display) -> ConnectError {
    ConnectError::new(ErrorCode::Unavailable, format!("NATS {subject}: {e}"))
}
