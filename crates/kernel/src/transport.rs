use connectrpc::RequestContext;

/// How a request reached the service. The NATS adapter stores [`Transport::Nats`]
/// as a request extension; when it is absent the request came over HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    Http,
    Nats { subject: String },
}

impl Transport {
    /// `"http"` or `"nats"`.
    pub fn name(&self) -> &'static str {
        match self {
            Transport::Http => "http",
            Transport::Nats { .. } => "nats",
        }
    }
}

/// Extra methods on `RequestContext` for handlers: `ctx.transport()`.
pub trait RequestContextExt {
    fn transport(&self) -> Transport;
}

impl RequestContextExt for RequestContext {
    fn transport(&self) -> Transport {
        self.extensions().get::<Transport>().cloned().unwrap_or(Transport::Http)
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
