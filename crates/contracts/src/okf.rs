//! OKF contract: append-only document storage (`okf.v1`). Mirrors `proto/okf/v1/okf.proto`.

use crate::{ProgressSender, RawRpcEndpoint, RpcError};
use async_trait::async_trait;
use prost::Message;
use std::sync::Arc;

/// One stored document. `id` is its file path inside the storage folder:
/// `<stream>/<yyyy-mm-dd>/<HHMMSS.micros>` in UTC server time, e.g. `screen-1.frames/2026-11-11/103045.123456`.
#[derive(Clone, PartialEq, Message)]
pub struct OkfDocument {
    /// Set by the server on Append. Leave empty when appending.
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub stream: String,
    /// Groups the documents of one activity. Chosen by the producer.
    #[prost(string, tag = "3")]
    pub correlation_id: String,
    /// Id of the document that caused this one. Optional.
    #[prost(string, tag = "4")]
    pub parent_id: String,
    /// Who produced it: `<module>/<instance>`.
    #[prost(string, tag = "5")]
    pub source: String,
    #[prost(string, tag = "6")]
    pub kind: String,
    /// Producer clock, microseconds since the Unix epoch. Informational only.
    #[prost(int64, tag = "7")]
    pub local_time_micros: i64,
    /// Server clock at append. Strictly increasing; this is the order.
    #[prost(int64, tag = "8")]
    pub server_time_micros: i64,
    #[prost(string, tag = "9")]
    pub content_type: String,
    #[prost(bytes = "vec", tag = "10")]
    pub data: Vec<u8>,
}

/// What Append returns for each document: the id and time the server gave it.
#[derive(Clone, PartialEq, Message)]
pub struct OkfDocumentRef {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(int64, tag = "2")]
    pub server_time_micros: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfAppendRequest {
    #[prost(message, repeated, tag = "1")]
    pub documents: Vec<OkfDocument>,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfAppendResponse {
    /// One entry per appended document, in the same order.
    #[prost(message, repeated, tag = "1")]
    pub refs: Vec<OkfDocumentRef>,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfGetRequest {
    #[prost(string, tag = "1")]
    pub id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfGetResponse {
    #[prost(message, optional, tag = "1")]
    pub document: Option<OkfDocument>,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfBatchGetRequest {
    #[prost(string, repeated, tag = "1")]
    pub ids: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfBatchGetResponse {
    #[prost(message, repeated, tag = "1")]
    pub documents: Vec<OkfDocument>,
    #[prost(string, repeated, tag = "2")]
    pub missing_ids: Vec<String>,
}

/// Documents with `from_micros <= server time < to_micros`. Empty `stream` = all streams; `to_micros` 0 = no end.
#[derive(Clone, PartialEq, Message)]
pub struct OkfRangeRequest {
    #[prost(string, tag = "1")]
    pub stream: String,
    #[prost(int64, tag = "2")]
    pub from_micros: i64,
    #[prost(int64, tag = "3")]
    pub to_micros: i64,
    /// 0 = 100. At most 1000.
    #[prost(uint32, tag = "4")]
    pub limit: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfRangeResponse {
    #[prost(message, repeated, tag = "1")]
    pub documents: Vec<OkfDocument>,
    /// Pass as `from_micros` to read the next page. 0 = this was the last page.
    #[prost(int64, tag = "2")]
    pub next_from_micros: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfTailRequest {
    #[prost(string, tag = "1")]
    pub stream: String,
    /// 0 = 100. At most 1000.
    #[prost(uint32, tag = "2")]
    pub limit: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct OkfTailResponse {
    #[prost(message, repeated, tag = "1")]
    pub documents: Vec<OkfDocument>,
}

/// Append-only writer: documents are saved once and never changed.
#[async_trait]
pub trait IOkfWriterService: Send + Sync + 'static {
    async fn append(&self, req: OkfAppendRequest, progress: ProgressSender) -> Result<OkfAppendResponse, RpcError>;
}

/// Reads documents by id or by server time. All lists are ordered oldest first.
#[async_trait]
pub trait IOkfReaderService: Send + Sync + 'static {
    async fn get(&self, req: OkfGetRequest, progress: ProgressSender) -> Result<OkfGetResponse, RpcError>;
    async fn batch_get(
        &self,
        req: OkfBatchGetRequest,
        progress: ProgressSender,
    ) -> Result<OkfBatchGetResponse, RpcError>;
    async fn range(&self, req: OkfRangeRequest, progress: ProgressSender) -> Result<OkfRangeResponse, RpcError>;
    async fn tail(&self, req: OkfTailRequest, progress: ProgressSender) -> Result<OkfTailResponse, RpcError>;
}

fn decode<M: Message + Default>(payload: &[u8]) -> Result<M, RpcError> {
    M::decode(payload).map_err(|e| RpcError::invalid_arg(e.to_string()))
}

pub struct OkfWriterEndpoint<T: IOkfWriterService>(pub Arc<T>);

#[async_trait]
impl<T: IOkfWriterService> RawRpcEndpoint for OkfWriterEndpoint<T> {
    fn service_name(&self) -> &'static str {
        "okf.v1.OkfWriterService"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["Append"]
    }

    async fn dispatch(&self, method: &str, payload: &[u8], progress: ProgressSender) -> Result<Vec<u8>, RpcError> {
        match method {
            "Append" => Ok(self.0.append(decode(payload)?, progress).await?.encode_to_vec()),
            _ => Err(RpcError::not_found(format!("unknown method {method}"))),
        }
    }
}

pub struct OkfReaderEndpoint<T: IOkfReaderService>(pub Arc<T>);

#[async_trait]
impl<T: IOkfReaderService> RawRpcEndpoint for OkfReaderEndpoint<T> {
    fn service_name(&self) -> &'static str {
        "okf.v1.OkfReaderService"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["Get", "BatchGet", "Range", "Tail"]
    }

    async fn dispatch(&self, method: &str, payload: &[u8], progress: ProgressSender) -> Result<Vec<u8>, RpcError> {
        match method {
            "Get" => Ok(self.0.get(decode(payload)?, progress).await?.encode_to_vec()),
            "BatchGet" => Ok(self.0.batch_get(decode(payload)?, progress).await?.encode_to_vec()),
            "Range" => Ok(self.0.range(decode(payload)?, progress).await?.encode_to_vec()),
            "Tail" => Ok(self.0.tail(decode(payload)?, progress).await?.encode_to_vec()),
            _ => Err(RpcError::not_found(format!("unknown method {method}"))),
        }
    }
}
