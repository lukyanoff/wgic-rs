pub mod fake_openai;

use sdk::state::{IStateStore, NatsKvStateStore};
use sdk::transport::{serve_endpoint, NatsLlmCallerClient, NatsRpcClient, NatsSensorClient};
use sdk::RawRpcEndpoint;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct TestEnv {
    pub nc: async_nats::Client,
    pub kv: async_nats::jetstream::kv::Store,
    pub prefix: String,
    handles: Vec<JoinHandle<()>>,
}

impl TestEnv {
    pub async fn connect() -> anyhow::Result<Self> {
        let nats_url = std::env::var("WGIC_NATS_URL")
            .unwrap_or_else(|_| "nats://127.0.0.1:4222".to_string());
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
        Ok(Self {
            nc,
            kv,
            prefix: format!("e2e_{short_id}"),
            handles: Vec::new(),
        })
    }

    pub fn instance_name(&self, base: &str) -> String {
        format!("{}_{}", self.prefix, base)
    }

    pub fn state_store<S>(&self, module: &str, instance: &str) -> Arc<dyn IStateStore<S>>
    where
        S: serde::Serialize
            + serde::de::DeserializeOwned
            + Clone
            + Default
            + Send
            + Sync
            + 'static,
    {
        Arc::new(NatsKvStateStore::new(
            self.kv.clone(),
            format!("{module}.{instance}"),
        ))
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
        NatsSensorClient(NatsRpcClient::new(self.nc.clone(), "sensor", instance))
    }

    pub fn llm_client(&self, instance: &str) -> NatsLlmCallerClient {
        NatsLlmCallerClient(NatsRpcClient::new(self.nc.clone(), "llm_caller", instance))
    }

    pub fn visioner_client(&self, instance: &str) -> NatsRpcClient {
        NatsRpcClient::new(self.nc.clone(), "visioner", instance)
    }

    pub fn okf_writer_client(&self, instance: &str) -> NatsRpcClient {
        NatsRpcClient::new(self.nc.clone(), "okf_writer", instance)
    }

    pub fn okf_reader_client(&self, instance: &str) -> NatsRpcClient {
        NatsRpcClient::new(self.nc.clone(), "okf_reader", instance)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        for h in &self.handles {
            h.abort();
        }
    }
}
