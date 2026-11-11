use crate::doc_id::{MAX_TIME_MICROS, check_stream, time_part};
use crate::storage::FileStore;
use async_trait::async_trait;
use contracts::{
    IOkfReaderService, OkfBatchGetRequest, OkfBatchGetResponse, OkfDocument, OkfGetRequest, OkfGetResponse,
    OkfRangeRequest, OkfRangeResponse, OkfTailRequest, OkfTailResponse,
};
use sdk::state::IStateStore;
use sdk::{ProgressSender, RpcError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;
/// A reply is one NATS message (1 MB). A page stops once it holds this much data.
const MAX_PAGE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkfReaderState {
    pub documents_read: u64,
}

pub struct OkfReaderHandler {
    store: Arc<FileStore>,
    state: Arc<dyn IStateStore<OkfReaderState>>,
}

/// Documents read for one reply. `complete` is false when the page filled up before the ids ran out.
struct Page {
    documents: Vec<OkfDocument>,
    complete: bool,
}

impl OkfReaderHandler {
    pub fn new(store: Arc<FileStore>, state: Arc<dyn IStateStore<OkfReaderState>>) -> Self {
        Self { store, state }
    }

    /// The one requested stream, or every stream when the request names none.
    fn streams_for(&self, stream: &str) -> Result<Vec<String>, RpcError> {
        if stream.is_empty() {
            return self.store.streams();
        }
        check_stream(stream)?;
        Ok(vec![stream.to_string()])
    }

    /// Reads documents in the order of `ids` until `limit` documents or `MAX_PAGE_BYTES` of data.
    fn read_page(&self, ids: &[String], limit: usize) -> Result<Page, RpcError> {
        let mut documents = Vec::new();
        let mut bytes = 0;
        for id in ids {
            if documents.len() >= limit || bytes >= MAX_PAGE_BYTES {
                return Ok(Page { documents, complete: false });
            }
            let Some(doc) = self.store.read(id)? else { continue };
            bytes += doc.data.len();
            documents.push(doc);
        }
        Ok(Page { documents, complete: true })
    }

    async fn count_read(&self, documents: usize) -> Result<(), RpcError> {
        let read = documents as u64;
        self.state
            .update_cas(Box::new(move |s| s.documents_read += read))
            .await
            .map_err(|e| RpcError::internal(format!("okf reader state CAS failed: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl IOkfReaderService for OkfReaderHandler {
    #[tracing::instrument(skip(self, req, _progress))]
    async fn get(&self, req: OkfGetRequest, _progress: ProgressSender) -> Result<OkfGetResponse, RpcError> {
        let Some(doc) = self.store.read(&req.id)? else {
            return Err(RpcError::not_found(format!("document '{}' not found", req.id)));
        };
        self.count_read(1).await?;
        Ok(OkfGetResponse { document: Some(doc) })
    }

    #[tracing::instrument(skip(self, req, progress))]
    async fn batch_get(
        &self,
        req: OkfBatchGetRequest,
        progress: ProgressSender,
    ) -> Result<OkfBatchGetResponse, RpcError> {
        progress.report(format!("reading {} documents", req.ids.len()));
        let mut documents = Vec::new();
        let mut missing_ids = Vec::new();
        for id in req.ids {
            match self.store.read(&id)? {
                Some(doc) => documents.push(doc),
                None => missing_ids.push(id),
            }
        }
        self.count_read(documents.len()).await?;
        Ok(OkfBatchGetResponse { documents, missing_ids })
    }

    #[tracing::instrument(skip(self, req, progress))]
    async fn range(&self, req: OkfRangeRequest, progress: ProgressSender) -> Result<OkfRangeResponse, RpcError> {
        let to_micros = if req.to_micros == 0 { MAX_TIME_MICROS } else { req.to_micros };

        let mut ids = Vec::new();
        for stream in self.streams_for(&req.stream)? {
            ids.extend(self.store.ids_between(&stream, req.from_micros, to_micros)?);
        }
        sort_by_time(&mut ids);

        progress.report(format!("{} documents in range", ids.len()));
        let page = self.read_page(&ids, page_limit(req.limit))?;
        self.count_read(page.documents.len()).await?;

        let next_from_micros = match page.documents.last() {
            Some(last) if !page.complete => last.server_time_micros + 1,
            _ => 0,
        };
        Ok(OkfRangeResponse { documents: page.documents, next_from_micros })
    }

    #[tracing::instrument(skip(self, req, _progress))]
    async fn tail(&self, req: OkfTailRequest, _progress: ProgressSender) -> Result<OkfTailResponse, RpcError> {
        let limit = page_limit(req.limit);

        let mut ids = Vec::new();
        for stream in self.streams_for(&req.stream)? {
            ids.extend(self.store.last_ids(&stream, limit)?);
        }
        sort_by_time(&mut ids);

        // Fill the page from the newest document backwards, then return it oldest first.
        ids.reverse();
        let mut page = self.read_page(&ids, limit)?;
        page.documents.reverse();

        self.count_read(page.documents.len()).await?;
        Ok(OkfTailResponse { documents: page.documents })
    }
}

/// Orders ids of different streams by server time. The writer never gives two documents the same time.
fn sort_by_time(ids: &mut [String]) {
    ids.sort_by(|a, b| time_part(a).cmp(time_part(b)));
}

fn page_limit(requested: u32) -> usize {
    if requested == 0 {
        return DEFAULT_LIMIT;
    }
    (requested as usize).min(MAX_LIMIT)
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
