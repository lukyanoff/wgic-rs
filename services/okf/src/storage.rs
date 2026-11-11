//! Plain files, written once. For the id `<stream>/<date>/<time>` the store keeps two files:
//! `<id>.json` (everything except the payload) and `<id>.data` (the payload bytes).

use crate::doc_id::{check_id, check_stream, format_id, time_part};
use contracts::OkfDocument;
use sdk::RpcError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

const DATE_LEN: usize = "yyyy-mm-dd".len();

/// The `<id>.json` file. A document shows up in listings only once this file exists.
#[derive(Serialize, Deserialize)]
struct Meta {
    id: String,
    stream: String,
    correlation_id: String,
    parent_id: String,
    source: String,
    kind: String,
    local_time_micros: i64,
    server_time_micros: i64,
    content_type: String,
}

/// Append-only document files under one root folder.
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Saves `doc` at the path given by `doc.id`. Fails if a document with that id already exists.
    pub fn write(&self, doc: &OkfDocument) -> Result<(), RpcError> {
        check_id(&doc.id)?;
        let meta = Meta {
            id: doc.id.clone(),
            stream: doc.stream.clone(),
            correlation_id: doc.correlation_id.clone(),
            parent_id: doc.parent_id.clone(),
            source: doc.source.clone(),
            kind: doc.kind.clone(),
            local_time_micros: doc.local_time_micros,
            server_time_micros: doc.server_time_micros,
            content_type: doc.content_type.clone(),
        };
        let meta_json = serde_json::to_vec_pretty(&meta).map_err(|e| RpcError::internal(e.to_string()))?;

        // Payload first: readers look for the .json file, so they never see a half-written document.
        write_once(&self.file_path(&doc.id, "data"), &doc.data)?;
        write_once(&self.file_path(&doc.id, "json"), &meta_json)
    }

    /// The document with this id, or `None` when there is no such file.
    pub fn read(&self, id: &str) -> Result<Option<OkfDocument>, RpcError> {
        check_id(id)?;
        let meta_json = match fs::read(self.file_path(id, "json")) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_error("read document", &e)),
        };
        let meta: Meta = serde_json::from_slice(&meta_json).map_err(|e| RpcError::internal(e.to_string()))?;
        let data = fs::read(self.file_path(id, "data")).map_err(|e| io_error("read document data", &e))?;

        Ok(Some(OkfDocument {
            id: meta.id,
            stream: meta.stream,
            correlation_id: meta.correlation_id,
            parent_id: meta.parent_id,
            source: meta.source,
            kind: meta.kind,
            local_time_micros: meta.local_time_micros,
            server_time_micros: meta.server_time_micros,
            content_type: meta.content_type,
            data,
        }))
    }

    /// Names of all streams that have a folder.
    pub fn streams(&self) -> Result<Vec<String>, RpcError> {
        let mut streams = sorted_names(&self.root)?;
        streams.retain(|name| check_stream(name).is_ok());
        Ok(streams)
    }

    /// Ids of `stream` with `from_micros <= server time < to_micros`, oldest first.
    pub fn ids_between(&self, stream: &str, from_micros: i64, to_micros: i64) -> Result<Vec<String>, RpcError> {
        let first = format_id(stream, from_micros)?;
        let end = format_id(stream, to_micros)?;
        let first_date = &time_part(&first)[..DATE_LEN];
        let end_date = &time_part(&end)[..DATE_LEN];

        let mut ids = Vec::new();
        for date in sorted_names(&self.root.join(stream))? {
            if date.as_str() < first_date || date.as_str() > end_date {
                continue;
            }
            ids.extend(self.ids_on(stream, &date)?);
        }
        // The first and last date folders can hold documents just outside the range.
        ids.retain(|id| *id >= first && *id < end);
        Ok(ids)
    }

    /// The newest `limit` ids of `stream`, oldest first.
    pub fn last_ids(&self, stream: &str, limit: usize) -> Result<Vec<String>, RpcError> {
        let mut newest_first = Vec::new();
        for date in sorted_names(&self.root.join(stream))?.iter().rev() {
            let ids = self.ids_on(stream, date)?;
            newest_first.extend(ids.into_iter().rev());
            if newest_first.len() >= limit {
                break;
            }
        }
        newest_first.truncate(limit);
        newest_first.reverse();
        Ok(newest_first)
    }

    /// Ids of the documents in one date folder, oldest first.
    fn ids_on(&self, stream: &str, date: &str) -> Result<Vec<String>, RpcError> {
        let mut ids = Vec::new();
        for name in sorted_names(&self.root.join(stream).join(date))? {
            if let Some(time) = name.strip_suffix(".json") {
                ids.push(format!("{stream}/{date}/{time}"));
            }
        }
        Ok(ids)
    }

    fn file_path(&self, id: &str, extension: &str) -> PathBuf {
        self.root.join(format!("{id}.{extension}"))
    }
}

/// Writes a new file that appears complete or not at all, and never replaces an existing one.
fn write_once(path: &Path, bytes: &[u8]) -> Result<(), RpcError> {
    let dir = path.parent().ok_or_else(|| RpcError::internal("document path has no folder"))?;
    fs::create_dir_all(dir).map_err(|e| io_error("create folder", &e))?;

    let mut file =
        tempfile::Builder::new().prefix(".tmp-").tempfile_in(dir).map_err(|e| io_error("create temp file", &e))?;
    file.write_all(bytes).map_err(|e| io_error("write file", &e))?;
    file.persist_noclobber(path).map_err(|e| io_error("save file", &e.error))?;
    Ok(())
}

/// File and folder names in `dir`, sorted. A missing folder is an empty list.
fn sorted_names(dir: &Path) -> Result<Vec<String>, RpcError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_error("list folder", &e)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_error("list folder", &e))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

fn io_error(action: &str, error: &std::io::Error) -> RpcError {
    RpcError::internal(format!("okf storage: {action} failed: {error}"))
}
