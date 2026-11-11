use crate::doc_id::{check_stream, format_id};
use crate::storage::FileStore;
use async_trait::async_trait;
use chrono::Utc;
use contracts::{IOkfWriterService, OkfAppendRequest, OkfAppendResponse, OkfDocument, OkfDocumentRef};
use sdk::state::IStateStore;
use sdk::{ProgressSender, RpcError};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Larger payloads would not fit into one NATS message (1 MB) together with a page of other documents.
const MAX_DATA_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkfWriterState {
    pub documents_appended: u64,
    pub last_id: String,
}

pub struct OkfWriterHandler {
    store: Arc<FileStore>,
    state: Arc<dyn IStateStore<OkfWriterState>>,
    /// Server time of the last appended document. Guarded so two calls never get the same time.
    last_time_micros: Mutex<i64>,
}

impl OkfWriterHandler {
    pub fn new(store: Arc<FileStore>, state: Arc<dyn IStateStore<OkfWriterState>>) -> Self {
        Self { store, state, last_time_micros: Mutex::new(0) }
    }

    /// The server clock, pushed forward by one microsecond when it would repeat or go back.
    /// This makes every document time unique, so the time alone is the order.
    fn next_time_micros(&self) -> i64 {
        let mut last = self.last_time_micros.lock().expect("clock lock is never held across a panic");
        *last = Utc::now().timestamp_micros().max(*last + 1);
        *last
    }
}

#[async_trait]
impl IOkfWriterService for OkfWriterHandler {
    #[tracing::instrument(skip(self, req, progress))]
    async fn append(&self, req: OkfAppendRequest, progress: ProgressSender) -> Result<OkfAppendResponse, RpcError> {
        if req.documents.is_empty() {
            return Err(RpcError::invalid_arg("documents must not be empty"));
        }
        // Check everything before the first write, so a bad document does not leave half a batch on disk.
        for doc in &req.documents {
            check_document(doc)?;
        }

        progress.report(format!("appending {} documents", req.documents.len()));
        let mut refs = Vec::with_capacity(req.documents.len());
        for mut doc in req.documents {
            doc.server_time_micros = self.next_time_micros();
            doc.id = format_id(&doc.stream, doc.server_time_micros)?;
            self.store.write(&doc)?;
            refs.push(OkfDocumentRef { id: doc.id, server_time_micros: doc.server_time_micros });
        }

        let appended = refs.len() as u64;
        let last_id = refs.last().map(|r| r.id.clone()).unwrap_or_default();
        self.state
            .update_cas(Box::new(move |s| {
                s.documents_appended += appended;
                s.last_id = last_id.clone();
            }))
            .await
            .map_err(|e| RpcError::internal(format!("okf writer state CAS failed: {e}")))?;

        progress.report("appended");
        tracing::info!(documents = appended, "okf documents appended");
        Ok(OkfAppendResponse { refs })
    }
}

fn check_document(doc: &OkfDocument) -> Result<(), RpcError> {
    check_stream(&doc.stream)?;
    if doc.data.len() > MAX_DATA_BYTES {
        return Err(RpcError::invalid_arg(format!(
            "data is {} bytes, the limit is {MAX_DATA_BYTES}; keep large files elsewhere and store a reference",
            doc.data.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
