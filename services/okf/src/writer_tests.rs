use super::*;
use sdk::state::InMemoryStateStore;
use tempfile::TempDir;

struct Fixture {
    // Keeps the folder alive; it is deleted when the test ends.
    _dir: TempDir,
    store: Arc<FileStore>,
    state: Arc<InMemoryStateStore<OkfWriterState>>,
    handler: OkfWriterHandler,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FileStore::new(dir.path()));
    let state = Arc::new(InMemoryStateStore::new(OkfWriterState::default()));
    let handler = OkfWriterHandler::new(store.clone(), state.clone());
    Fixture { _dir: dir, store, state, handler }
}

fn document(stream: &str, text: &str) -> OkfDocument {
    OkfDocument {
        stream: stream.into(),
        correlation_id: "run-1".into(),
        kind: "note".into(),
        content_type: "text/plain".into(),
        data: text.as_bytes().to_vec(),
        ..Default::default()
    }
}

async fn append(f: &Fixture, documents: Vec<OkfDocument>) -> Result<OkfAppendResponse, RpcError> {
    f.handler.append(OkfAppendRequest { documents }, ProgressSender::noop()).await
}

#[tokio::test]
async fn appended_document_can_be_read_back_by_its_id() {
    let f = fixture();

    let res = append(&f, vec![document("notes", "hello")]).await.unwrap();

    let saved = f.store.read(&res.refs[0].id).unwrap().unwrap();
    assert_eq!(saved.data, b"hello");
    assert_eq!(saved.correlation_id, "run-1");
    assert_eq!(saved.server_time_micros, res.refs[0].server_time_micros);
    assert!(saved.id.starts_with("notes/"));
}

#[tokio::test]
async fn ids_of_a_batch_are_strictly_increasing_across_streams() {
    let f = fixture();
    let mut documents = Vec::new();
    for i in 0..50 {
        documents.push(document(if i % 2 == 0 { "a" } else { "b" }, "x"));
    }

    let res = append(&f, documents).await.unwrap();

    for pair in res.refs.windows(2) {
        assert!(pair[0].server_time_micros < pair[1].server_time_micros);
    }
}

#[tokio::test]
async fn append_updates_cas_state() {
    let f = fixture();

    let res = append(&f, vec![document("notes", "one"), document("notes", "two")]).await.unwrap();

    let state = f.state.get().await.unwrap();
    assert_eq!(state.documents_appended, 2);
    assert_eq!(state.last_id, res.refs[1].id);
}

#[tokio::test]
async fn empty_batch_is_invalid_argument() {
    let f = fixture();

    let err = append(&f, vec![]).await.unwrap_err();

    assert_eq!(err.code, "invalid_argument");
}

#[tokio::test]
async fn bad_stream_name_rejects_the_whole_batch() {
    let f = fixture();

    let err = append(&f, vec![document("notes", "ok"), document("../escape", "bad")]).await.unwrap_err();

    assert_eq!(err.code, "invalid_argument");
    assert!(f.store.streams().unwrap().is_empty());
}

#[tokio::test]
async fn oversized_data_is_invalid_argument() {
    let f = fixture();
    let mut doc = document("notes", "");
    doc.data = vec![0; MAX_DATA_BYTES + 1];

    let err = append(&f, vec![doc]).await.unwrap_err();

    assert_eq!(err.code, "invalid_argument");
}
