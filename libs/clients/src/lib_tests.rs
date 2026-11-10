//! The factories reach the instance they are asked for. Two fake greet "instances" answer with their own
//! name, so a call shows where it went.

use std::sync::Arc;
use std::time::Duration;

use connectrpc::{RequestContext, Response, Router, ServiceRequest, ServiceResult};
use contracts::proto::greet::v1::{GreetRequest, GreetResponse, GreetService};
use kernel::{Deps, Module};

use super::*;

/// A greet service that answers "<instance>: <name>".
struct NamedGreeter(&'static str);

#[allow(refining_impl_trait)]
impl GreetService for NamedGreeter {
    async fn greet(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GreetRequest>,
    ) -> ServiceResult<GreetResponse> {
        Response::ok(GreetResponse { greeting: format!("{}: {}", self.0, request.name), ..Default::default() })
    }
}

impl Module for NamedGreeter {
    fn name(&self) -> &'static str {
        "greet"
    }

    fn register(&self, router: Router) -> Router {
        router.add_service(Arc::new(NamedGreeter(self.0)))
    }
}

fn deps() -> Deps {
    let a: Arc<dyn Module> = Arc::new(NamedGreeter("a"));
    let b: Arc<dyn Module> = Arc::new(NamedGreeter("b"));
    Deps::in_process_instances(vec![("greet-a", vec![a]), ("greet-b", vec![b])])
}

async fn greet(deps: &Deps, instance: &str) -> Result<String, connectrpc::ConnectError> {
    let client = GreetClients::new(deps).get(instance, Duration::from_secs(5));
    let request = GreetRequest { name: "Jane".into(), ..Default::default() };
    Ok(client.greet(request).await?.into_owned().greeting)
}

#[tokio::test]
async fn each_instance_id_reaches_its_own_service() {
    let deps = deps();
    assert_eq!(greet(&deps, "greet-a").await.unwrap(), "a: Jane");
    assert_eq!(greet(&deps, "greet-b").await.unwrap(), "b: Jane");
}

#[tokio::test]
async fn unknown_instance_is_unimplemented() {
    let error = greet(&deps(), "nobody").await.unwrap_err();
    assert_eq!(error.code, connectrpc::ErrorCode::Unimplemented);
}
