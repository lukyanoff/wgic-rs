//! Proto ⇄ domain. Keep this thin: decode, call the domain, log/measure, encode.

use connectrpc::{
    ConnectError, ErrorCode, RequestContext, Response, ServiceRequest, ServiceResult,
};
use contracts::proto::greet::v1::{GreetRequest, GreetResponse, GreetService};
use kernel::{Deps, Logger, Metrics, RequestContextExt, kv};

use crate::domain::{self, GreetError};

/// Logger and metrics are injected through `new` (like constructor injection in C#).
pub struct GreetHandler {
    log: Logger,
    metrics: Metrics,
}

impl GreetHandler {
    pub fn new(deps: &Deps) -> Self {
        Self {
            log: deps.logger("greet"),
            metrics: deps.metrics("greet"),
        }
    }
}

#[allow(refining_impl_trait)]
impl GreetService for GreetHandler {
    async fn greet(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GreetRequest>,
    ) -> ServiceResult<GreetResponse> {
        let transport = ctx.transport().name();
        let greeting = match domain::greeting(request.name) {
            Ok(greeting) => greeting,
            Err(error) => {
                self.log
                    .warn("greeting rejected", [kv("reason", reason(&error))]);
                self.metrics
                    .count("rejected", 1, [kv("reason", reason(&error))]);
                return Err(to_connect_error(error));
            }
        };
        // Logs carry trace_id/span_id automatically (we run inside the rpc.server span).
        self.log.info(
            "greeting sent",
            [kv("name", request.name), kv("transport", transport)],
        );
        self.metrics
            .count("greetings", 1, [kv("transport", transport)]);
        Response::ok(GreetResponse {
            greeting,
            ..Default::default()
        })
    }
}

fn to_connect_error(error: GreetError) -> ConnectError {
    ConnectError::new(ErrorCode::InvalidArgument, error.to_string())
}

/// Short, stable label for logs and the `greet.rejected` metric.
fn reason(error: &GreetError) -> &'static str {
    match error {
        GreetError::EmptyName => "empty_name",
        GreetError::NameTooLong => "name_too_long",
    }
}
