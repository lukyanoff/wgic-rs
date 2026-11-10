use std::sync::Arc;

use connect_rust_example::proto::greet::v1::{GreetRequest, GreetResponse, GreetService};
use connect_rust_example::{RequestContextOtel, init_telemetry, run};
use connectrpc::{RequestContext, Response, Router, ServiceRequest, ServiceResult};

struct GreetServer;

#[allow(refining_impl_trait)]
impl GreetService for GreetServer {
    async fn greet(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GreetRequest>,
    ) -> ServiceResult<GreetResponse> {
        // Runs inside the rpc.server span: this log carries trace_id/span_id and is
        // attached to the span as an event. `ctx.otel()` is the span's OTEL context.
        tracing::info!(
            name = %request.name,
            transport = ctx.transport().name(),
            trace_id = ctx.trace_id().unwrap_or_default(),
            "greeting"
        );
        Response::ok(GreetResponse {
            greeting: format!("Hello, {}!", request.name),
            ..Default::default()
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _telemetry = init_telemetry("greet-server");
    let router = Router::new().add_service(Arc::new(GreetServer));
    if let Err(e) = run(router).await {
        tracing::error!(error = %e, "server stopped");
        return Err(e);
    }
    Ok(())
}
