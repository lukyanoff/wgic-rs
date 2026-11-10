//! Adapters expose modules to the world. Add a new transport by implementing [`Adapter`].

use std::net::SocketAddr;
use std::sync::Arc;

use connectrpc::Router;
use futures::future::{BoxFuture, try_join_all};
use kernel::Module;

use crate::http_server::serve_http;
use crate::nats::{NatsConfig, serve_nats};
use crate::traced_service;

/// A way to expose modules (HTTP, NATS, ...). Runs until the transport stops.
pub trait Adapter: Send + Sync {
    /// `"http"`, `"nats"`, ... for logs.
    fn name(&self) -> &'static str;

    fn serve(&self, modules: Vec<Arc<dyn Module>>) -> BoxFuture<'static, anyhow::Result<()>>;
}

/// All modules in one router.
fn router_for(modules: &[Arc<dyn Module>]) -> Router {
    modules.iter().fold(Router::new(), |router, module| module.register(router))
}

/// Connect, gRPC and gRPC-web on one HTTP port, for all modules together.
pub struct HttpAdapter {
    pub addr: SocketAddr,
}

impl Adapter for HttpAdapter {
    fn name(&self) -> &'static str {
        "http"
    }

    fn serve(&self, modules: Vec<Arc<dyn Module>>) -> BoxFuture<'static, anyhow::Result<()>> {
        let service = traced_service(router_for(&modules));
        Box::pin(serve_http(service, self.addr))
    }
}

/// One NATS micro service per module (named after the module), so
/// `nats micro ls` shows each service and its instances separately.
pub struct NatsAdapter {
    pub client: async_nats::Client,
    pub config: NatsConfig,
}

impl Adapter for NatsAdapter {
    fn name(&self) -> &'static str {
        "nats"
    }

    fn serve(&self, modules: Vec<Arc<dyn Module>>) -> BoxFuture<'static, anyhow::Result<()>> {
        let runs = modules.into_iter().map(|module| {
            let router = module.register(Router::new());
            let methods: Vec<String> = router.methods().map(str::to_string).collect();
            let (client, config) = (self.client.clone(), self.config.clone());
            async move {
                let service = traced_service(router);
                serve_nats(service, methods, client, &config, module.name()).await
            }
        });
        let all = try_join_all(runs);
        Box::pin(async move { all.await.map(|_| ()) })
    }
}
