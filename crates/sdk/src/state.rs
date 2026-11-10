use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Mutex;

pub type StateMutator<S> = Box<dyn Fn(&mut S) + Send + Sync + 'static>;

#[async_trait]
pub trait IStateStore<S>: Send + Sync
where
    S: Serialize + DeserializeOwned + Clone + Default + Send + Sync + 'static,
{
    async fn get(&self) -> anyhow::Result<S>;
    async fn update_cas(&self, mutator: StateMutator<S>) -> anyhow::Result<S>;
}

/// NATS KV Compare-And-Swap implementation for multi-replica HA state.
pub struct NatsKvStateStore<S> {
    kv: async_nats::jetstream::kv::Store,
    key: String,
    _marker: std::marker::PhantomData<S>,
}

impl<S> NatsKvStateStore<S> {
    pub fn new(kv: async_nats::jetstream::kv::Store, instance_key: impl Into<String>) -> Self {
        Self {
            kv,
            key: instance_key.into(),
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<S> IStateStore<S> for NatsKvStateStore<S>
where
    S: Serialize + DeserializeOwned + Clone + Default + Send + Sync + 'static,
{
    async fn get(&self) -> anyhow::Result<S> {
        match self.kv.entry(&self.key).await? {
            Some(entry) if !entry.value.is_empty() => Ok(serde_json::from_slice(&entry.value)?),
            _ => Ok(S::default()),
        }
    }

    async fn update_cas(&self, mutator: StateMutator<S>) -> anyhow::Result<S> {
        for _ in 0..10 {
            let (mut current, rev) = match self.kv.entry(&self.key).await? {
                Some(e) if !e.value.is_empty() => {
                    (serde_json::from_slice::<S>(&e.value)?, e.revision)
                }
                _ => (S::default(), 0),
            };
            mutator(&mut current);
            let bytes = serde_json::to_vec(&current)?.into();

            let succeeded = if rev == 0 {
                self.kv.create(&self.key, bytes).await.is_ok()
            } else {
                self.kv.update(&self.key, bytes, rev).await.is_ok()
            };
            if succeeded {
                return Ok(current);
            }
        }
        anyhow::bail!("CAS conflict limit reached on key {}", self.key)
    }
}

/// In-memory state store for fast unit testing without NATS.
pub struct InMemoryStateStore<S> {
    inner: Mutex<S>,
}

impl<S: Default> InMemoryStateStore<S> {
    pub fn new(initial: S) -> Self {
        Self {
            inner: Mutex::new(initial),
        }
    }
}

#[async_trait]
impl<S> IStateStore<S> for InMemoryStateStore<S>
where
    S: Serialize + DeserializeOwned + Clone + Default + Send + Sync + 'static,
{
    async fn get(&self) -> anyhow::Result<S> {
        Ok(self.inner.lock().unwrap().clone())
    }

    async fn update_cas(&self, mutator: StateMutator<S>) -> anyhow::Result<S> {
        let mut guard = self.inner.lock().unwrap();
        mutator(&mut *guard);
        let updated = guard.clone();
        drop(guard);
        Ok(updated)
    }
}
