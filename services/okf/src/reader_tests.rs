use super::*;
use crate::doc_id::format_id;
use sdk::state::InMemoryStateStore;
use tempfile::TempDir;

// 2026-11-11T10:30:45Z
const T0: i64 = 1_794_393_045_000_000;
const DAY: i64 = 86_400_000_000;

struct Fixture {
    // Keeps the folder alive; it is deleted when the test ends.
    _dir: TempDir,
    store: Arc<FileStore>,
    state: Arc<InMemoryStateStore<OkfReaderState>>,
    handler: OkfReaderHandler,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FileStore::new(dir.path()));
    let state = Arc::new(InMemoryStateStore::new(OkfReaderState::default()));
    let handler = OkfReaderHandler::new(store.clone(), state.clone());
    Fixture { _dir: dir, store, state, handler }
}

/// Saves a document straight into the store at a chosen server time and returns its id.
fn save(f: &Fixture, stream: &str, time_micros: i64, data: Vec<u8>) -> String {
    let id = format_id(stream, time_micros).unwrap();
    let doc = OkfDocument {
        id: id.clone(),
        stream: stream.into(),
        server_time_micros: time_micros,
        data,
        ..Default::default()
    };
    f.store.write(&doc).unwrap();
    id
}

fn times(documents: &[OkfDocument]) -> Vec<i64> {
    documents.iter().map(|d| d.server_time_micros - T0).collect()
}

async fn range(f: &Fixture, req: OkfRangeRequest) -> OkfRangeResponse {
    f.handler.range(req, ProgressSender::noop()).await.unwrap()
}

async fn tail(f: &Fixture, stream: &str, limit: u32) -> Vec<OkfDocument> {
    f.handler.tail(OkfTailRequest { stream: stream.into(), limit }, ProgressSender::noop()).await.unwrap().documents
}

#[tokio::test]
async fn get_returns_the_document_and_counts_it_in_state() {
    let f = fixture();
    let id = save(&f, "notes", T0, b"hello".to_vec());

    let res = f.handler.get(OkfGetRequest { id: id.clone() }, ProgressSender::noop()).await.unwrap();

    let doc = res.document.unwrap();
    assert_eq!(doc.id, id);
    assert_eq!(doc.data, b"hello");
    assert_eq!(f.state.get().await.unwrap().documents_read, 1);
}

#[tokio::test]
async fn get_of_unknown_id_is_not_found() {
    let f = fixture();
    let id = format_id("notes", T0).unwrap();

    let err = f.handler.get(OkfGetRequest { id }, ProgressSender::noop()).await.unwrap_err();

    assert_eq!(err.code, "not_found");
}

#[tokio::test]
async fn get_of_a_path_outside_the_folder_is_invalid_argument() {
    let f = fixture();

    let err = f.handler.get(OkfGetRequest { id: "../../etc/passwd".into() }, ProgressSender::noop()).await;

    assert_eq!(err.unwrap_err().code, "invalid_argument");
}

#[tokio::test]
async fn batch_get_splits_found_and_missing() {
    let f = fixture();
    let found = save(&f, "notes", T0, vec![1]);
    let missing = format_id("notes", T0 + 1).unwrap();

    let req = OkfBatchGetRequest { ids: vec![found.clone(), missing.clone()] };
    let res = f.handler.batch_get(req, ProgressSender::noop()).await.unwrap();

    assert_eq!(res.documents.len(), 1);
    assert_eq!(res.documents[0].id, found);
    assert_eq!(res.missing_ids, vec![missing]);
}

#[tokio::test]
async fn range_includes_from_and_excludes_to() {
    let f = fixture();
    for offset in [0, 10, 20, 30] {
        save(&f, "notes", T0 + offset, vec![]);
    }

    let req = OkfRangeRequest { stream: "notes".into(), from_micros: T0 + 10, to_micros: T0 + 30, limit: 0 };
    let res = range(&f, req).await;

    assert_eq!(times(&res.documents), vec![10, 20]);
    assert_eq!(res.next_from_micros, 0);
}

#[tokio::test]
async fn range_spans_days_in_time_order() {
    let f = fixture();
    save(&f, "notes", T0 + 2 * DAY, vec![]);
    save(&f, "notes", T0, vec![]);
    save(&f, "notes", T0 + DAY, vec![]);

    let req = OkfRangeRequest { stream: "notes".into(), from_micros: T0, to_micros: 0, limit: 0 };
    let res = range(&f, req).await;

    assert_eq!(times(&res.documents), vec![0, DAY, 2 * DAY]);
}

#[tokio::test]
async fn range_without_stream_merges_all_streams_by_time() {
    let f = fixture();
    save(&f, "b", T0 + 1, vec![]);
    save(&f, "a", T0 + 2, vec![]);
    save(&f, "b", T0 + 3, vec![]);

    let res = range(&f, OkfRangeRequest { from_micros: T0, ..Default::default() }).await;

    assert_eq!(times(&res.documents), vec![1, 2, 3]);
}

#[tokio::test]
async fn range_pages_with_next_from_until_the_end() {
    let f = fixture();
    for offset in 0..5 {
        save(&f, "notes", T0 + offset, vec![]);
    }

    let mut seen = Vec::new();
    let mut from_micros = T0;
    let mut pages = 0;
    while from_micros != 0 {
        let req = OkfRangeRequest { stream: "notes".into(), from_micros, to_micros: 0, limit: 2 };
        let res = range(&f, req).await;
        seen.extend(times(&res.documents));
        from_micros = res.next_from_micros;
        pages += 1;
    }

    assert_eq!(seen, vec![0, 1, 2, 3, 4]);
    assert_eq!(pages, 3);
}

#[tokio::test]
async fn range_page_stops_at_the_byte_limit() {
    let f = fixture();
    for offset in 0..3 {
        save(&f, "notes", T0 + offset, vec![0; MAX_PAGE_BYTES / 2]);
    }

    let res = range(&f, OkfRangeRequest { stream: "notes".into(), from_micros: T0, ..Default::default() }).await;

    assert_eq!(times(&res.documents), vec![0, 1]);
    assert_eq!(res.next_from_micros, T0 + 2);
}

#[tokio::test]
async fn tail_returns_the_newest_documents_oldest_first() {
    let f = fixture();
    save(&f, "notes", T0, vec![]);
    save(&f, "notes", T0 + DAY, vec![]);
    save(&f, "notes", T0 + DAY + 1, vec![]);

    let documents = tail(&f, "notes", 2).await;

    assert_eq!(times(&documents), vec![DAY, DAY + 1]);
}

#[tokio::test]
async fn tail_without_stream_merges_all_streams_by_time() {
    let f = fixture();
    save(&f, "a", T0 + 1, vec![]);
    save(&f, "b", T0 + 2, vec![]);
    save(&f, "a", T0 + 3, vec![]);

    let documents = tail(&f, "", 2).await;

    assert_eq!(times(&documents), vec![2, 3]);
}

#[tokio::test]
async fn tail_of_an_empty_store_is_empty() {
    let f = fixture();

    assert!(tail(&f, "", 10).await.is_empty());
}
