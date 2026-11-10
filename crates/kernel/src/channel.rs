use std::error::Error;
use std::sync::Arc;

use bytes::Bytes;
use connectrpc::client::{ClientBody, ClientTransport, ServiceTransport};
use connectrpc::{ConnectError, ConnectRpcService, Router};
use futures::future::BoxFuture;
use http::{Request, Response};
use http_body::Body;
use http_body_util::BodyExt;
use http_body_util::combinators::UnsyncBoxBody;

/// Response body of a [`Channel`].
pub type ChannelBody = UnsyncBoxBody<Bytes, ConnectError>;

type SendResult = Result<Response<ChannelBody>, ConnectError>;
type SendFn = dyn Fn(Request<ClientBody>) -> BoxFuture<'static, SendResult> + Send + Sync;

/// A client transport with one concrete type, whatever is underneath (HTTP, NATS, in-process).
///
/// Generated clients are generic over their transport (`GreetServiceClient<T>`).
/// Services always use `GreetServiceClient<Channel>`, so they never see transport types.
#[derive(Clone)]
pub struct Channel {
    send: Arc<SendFn>,
}

impl Channel {
    /// Wrap any connectrpc client transport.
    pub fn new<T>(transport: T) -> Self
    where
        T: ClientTransport,
        <T::ResponseBody as Body>::Error: Error + Send + Sync + 'static,
    {
        let send = move |request: Request<ClientBody>| -> BoxFuture<'static, SendResult> {
            let pending = transport.send(request);
            Box::pin(async move {
                let response = pending.await.map_err(to_connect_error)?;
                Ok(response.map(|body| body.map_err(to_connect_error).boxed_unsync()))
            })
        };
        Self { send: Arc::new(send) }
    }

    /// Call the services in `router` directly in this process: the full connect
    /// stack (codecs, errors, headers) but no network. Used by integration tests.
    pub fn in_process(router: Router) -> Self {
        Self::new(ServiceTransport::new(ConnectRpcService::new(router)))
    }
}

/// Makes a [`Channel`] to one named service instance (≈ C# `IChannelFactory`).
///
/// Over NATS the instance id is the subject prefix of the target host (`screen-1`, `llm-claude`), so
/// `channel_to("screen-1")` reaches the host started with `NATS_PREFIX=screen-1`.
pub trait ChannelFactory: Send + Sync {
    /// A channel whose calls go to `instance`.
    fn channel_to(&self, instance: &str) -> Channel;
}

/// The default factory: one channel for every id (HTTP to `UPSTREAM_ADDR`, or a single in-process router).
pub(crate) struct SameChannel(pub Channel);

impl ChannelFactory for SameChannel {
    fn channel_to(&self, _instance: &str) -> Channel {
        self.0.clone()
    }
}

impl ClientTransport for Channel {
    type ResponseBody = ChannelBody;
    type Error = ConnectError;

    fn send(&self, request: Request<ClientBody>) -> BoxFuture<'static, SendResult> {
        (self.send)(request)
    }
}

/// Keep a `ConnectError` as is (e.g. `deadline_exceeded` from NATS); wrap anything else.
fn to_connect_error<E: Error + Send + Sync + 'static>(error: E) -> ConnectError {
    let boxed: Box<dyn Error + Send + Sync> = Box::new(error);
    match boxed.downcast::<ConnectError>() {
        Ok(connect_error) => *connect_error,
        Err(other) => ConnectError::unavailable_from_transport("transport error", other),
    }
}

#[cfg(test)]
#[path = "channel_tests.rs"]
mod tests;
