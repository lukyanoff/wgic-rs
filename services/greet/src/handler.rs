//! `greet.v1.GreetService/Greet`: check the name, answer "Hello, <name>!", then log and count the greeting.
//! Failed calls need no code here: the platform logs and measures every RPC with its error code.

// ---------------------------------------------------------------------------------------------
// Usings
// ---------------------------------------------------------------------------------------------

use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceResult};
use contracts::proto::greet::v1::{GreetRequest, GreetResponse, GreetService};
use kernel::{Deps, Logger, Metrics, RequestContextExt, kv};

/// Longest name accepted, in characters (not bytes).
const MAX_NAME_CHARS: usize = 100;

// ---------------------------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------------------------

/// ≈ `class GreetHandler : GreetService.GreetServiceBase`, with logger and metrics injected in the constructor.
pub struct GreetHandler {
    log: Logger,
    metrics: Metrics,
}

impl GreetHandler {
    pub fn new(deps: &Deps) -> Self {
        Self { log: deps.logger("greet"), metrics: deps.metrics("greet") }
    }
}

// `#[allow(refining_impl_trait)]`: our async methods are a bit more specific than the generated trait asks for.
#[allow(refining_impl_trait)]
impl GreetService for GreetHandler {
    async fn greet(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GreetRequest>,
    ) -> ServiceResult<GreetResponse> {
        // `?` hands the error straight back to the client (≈ `throw new RpcException(...)`).
        let greeting = greeting(request.name)?;
        let transport = ctx.transport().name();
        self.log.info("greeting sent", [kv("name", request.name), kv("transport", transport)]);
        self.metrics.count("greetings", 1, [kv("transport", transport)]);
        Response::ok(GreetResponse { greeting, ..Default::default() })
    }
}

// ---------------------------------------------------------------------------------------------
// Rules (pure, unit tested)
// ---------------------------------------------------------------------------------------------

/// `" Jane "` → `"Hello, Jane!"`. An empty or too long name is `invalid_argument`.
fn greeting(name: &str) -> Result<String, ConnectError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ConnectError::invalid_argument("name must not be empty"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ConnectError::invalid_argument(format!("name must be at most {MAX_NAME_CHARS} characters")));
    }
    Ok(format!("Hello, {name}!"))
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
