# OKF Service Box Rules
- Pure domain crate: depends ONLY on `contracts`, `sdk`, `chrono`, `serde` and `tempfile`.
- Two handlers over one folder: `OkfWriterHandler` (append-only) and `OkfReaderHandler`. Run exactly ONE writer per
  folder: it owns the clock that keeps ids strictly ordered. Readers can have many replicas.
- A document id is its file path: `<stream>/<yyyy-mm-dd>/<HHMMSS.micros>` (UTC server time). Never change this format;
  sorting ids as text must stay equal to sorting by time.
- Files are written once through `tempfile` + `persist_noclobber`. Never overwrite or delete a document.
- Inject `Arc<FileStore>` and `Arc<dyn IStateStore<...>>` via `new(...)`. Unit tests use `tempfile::tempdir()`.
