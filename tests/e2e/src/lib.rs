pub mod fake_openai;

use sdk::RawRpcEndpoint;
use sdk::state::{IStateStore, NatsKvStateStore};
use sdk::{NatsLlmCallerClient, NatsRpcClient, NatsSensorClient, serve_endpoint};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

/// Per-call timeout of the test clients.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

pub struct TestEnv {
    pub nc: async_nats::Client,
    pub kv: async_nats::jetstream::kv::Store,
    pub prefix: String,
    handles: Vec<JoinHandle<()>>,
}

impl TestEnv {
    pub async fn connect() -> anyhow::Result<Self> {
        let nats_url = std::env::var("WGIC_NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_string());
        let nc = async_nats::connect(&nats_url).await?;
        let js = async_nats::jetstream::new(nc.clone());

        let kv = js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "wgic_e2e_state".into(),
                history: 1,
                ..Default::default()
            })
            .await?;

        let short_id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        Ok(Self { nc, kv, prefix: format!("e2e_{short_id}"), handles: Vec::new() })
    }

    pub fn instance_name(&self, base: &str) -> String {
        format!("{}_{}", self.prefix, base)
    }

    pub fn state_store<S>(&self, module: &str, instance: &str) -> Arc<dyn IStateStore<S>>
    where
        S: serde::Serialize + serde::de::DeserializeOwned + Clone + Default + Send + Sync + 'static,
    {
        Arc::new(NatsKvStateStore::new(self.kv.clone(), format!("{module}.{instance}")))
    }

    pub async fn mount_endpoint(
        &mut self,
        module: &str,
        instance: &str,
        endpoint: Arc<dyn RawRpcEndpoint>,
    ) -> anyhow::Result<()> {
        let mut h = serve_endpoint(self.nc.clone(), module, instance, endpoint).await?;
        self.handles.append(&mut h);
        self.nc.flush().await?;
        Ok(())
    }

    pub fn sensor_client(&self, instance: &str) -> NatsSensorClient {
        NatsSensorClient(NatsRpcClient::new(self.nc.clone(), "sensor", instance, CALL_TIMEOUT))
    }

    pub fn llm_client(&self, instance: &str) -> NatsLlmCallerClient {
        NatsLlmCallerClient(NatsRpcClient::new(self.nc.clone(), "llm_caller", instance, CALL_TIMEOUT))
    }

    pub fn visioner_client(&self, instance: &str) -> NatsRpcClient {
        NatsRpcClient::new(self.nc.clone(), "visioner", instance, CALL_TIMEOUT)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        for h in &self.handles {
            h.abort();
        }
    }
}
