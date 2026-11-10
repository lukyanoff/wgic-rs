use std::sync::Arc;

use connect_rust_example::proto::greet::v1::{GreetRequest, GreetResponse, GreetService};
use connect_rust_example::run;
use connectrpc::{RequestContext, Response, Router, ServiceRequest, ServiceResult};

struct GreetServer;

#[allow(refining_impl_trait)]
impl GreetService for GreetServer {
    async fn greet(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GreetRequest>,
    ) -> ServiceResult<GreetResponse> {
        Response::ok(GreetResponse {
            greeting: format!("Hello, {}!", request.name),
            ..Default::default()
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let router = Router::new().add_service(Arc::new(GreetServer));
    run(router).await
}
