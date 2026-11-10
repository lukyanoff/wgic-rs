//! How a service plugs into the host.

use std::sync::Arc;

use connectrpc::{Router, ServiceRegister};

use crate::Deps;

/// A service as the host sees it (≈ a C# `IHostedService` registration): a name + its RPC handlers.
/// Services don't implement it by hand; they call [`module`].
pub trait Module: Send + Sync + 'static {
    /// Short, stable name: used by `SERVICES=<name>,...` and as the NATS micro service name.
    fn name(&self) -> &'static str;

    /// Add this service's RPC handlers to `router` and return it.
    fn register(&self, router: Router) -> Router;
}

/// Builds one service from the host's deps. The host lists them as `("greet", greet::module)`.
pub type ModuleFactory = fn(&Deps) -> Arc<dyn Module>;

/// Wrap a handler (anything implementing a generated `XxxService` trait) as a [`Module`]:
///
/// ```ignore
/// pub fn module(deps: &Deps) -> Arc<dyn Module> {
///     kernel::module("greet", GreetHandler::new(deps))
/// }
/// ```
pub fn module<S, Marker>(name: &'static str, handler: S) -> Arc<dyn Module>
where
    S: Send + Sync + 'static,
    Arc<S>: ServiceRegister<Marker>,
    Marker: 'static,
{
    let handler = Arc::new(handler);
    Arc::new(HandlerModule { name, register: Box::new(move |router: Router| router.add_service(handler.clone())) })
}

struct HandlerModule {
    name: &'static str,
    register: Box<dyn Fn(Router) -> Router + Send + Sync>,
}

impl Module for HandlerModule {
    fn name(&self) -> &'static str {
        self.name
    }

    fn register(&self, router: Router) -> Router {
        (self.register)(router)
    }
}
