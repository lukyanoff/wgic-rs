//! Proto ⇄ domain. Keep this thin: decode, call the domain, encode.

use connectrpc::{
    ConnectError, ErrorCode, RequestContext, Response, ServiceRequest, ServiceResult,
};
use contracts::proto::greet::v1::{GreetRequest, GreetResponse, GreetService};
use kernel::RequestContextExt;

use crate::domain::{self, GreetError};

pub struct GreetHandler;

#[allow(refining_impl_trait)]
impl GreetService for GreetHandler {
    async fn greet(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GreetRequest>,
    ) -> ServiceResult<GreetResponse> {
        // Logs here carry trace_id/span_id automatically (we run inside the rpc.server span).
        tracing::info!(name = %request.name, transport = ctx.transport().name(), "greeting");
        let greeting = domain::greeting(request.name).map_err(to_connect_error)?;
        Response::ok(GreetResponse {
            greeting,
            ..Default::default()
        })
    }
}

fn to_connect_error(error: GreetError) -> ConnectError {
    ConnectError::new(ErrorCode::InvalidArgument, error.to_string())
}
