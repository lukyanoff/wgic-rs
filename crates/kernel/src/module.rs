use connectrpc::Router;

/// A service plugs itself into the host by implementing this trait.
///
/// Think of it as an interface (C#) / protocol (Python): the host only sees
/// `Vec<Box<dyn Module>>` and never knows which services exist.
pub trait Module: Send + Sync + 'static {
    /// Short, stable name: used by `SERVICES=<name>,...` and as the NATS micro service name.
    fn name(&self) -> &'static str;

    /// Add this service's RPC handlers to `router` and return it.
    fn register(&self, router: Router) -> Router;
}
