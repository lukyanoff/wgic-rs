use contracts::{
    OkfAppendRequest, OkfAppendResponse, OkfDocument, OkfGetRequest, OkfGetResponse, OkfRangeRequest, OkfRangeResponse,
    OkfReaderEndpoint, OkfTailRequest, OkfTailResponse, OkfWriterEndpoint,
};
use e2e::TestEnv;
use okf::{FileStore, OkfReaderHandler, OkfReaderState, OkfWriterHandler, OkfWriterState};
use sdk::{NatsRpcClient, ProgressSender};
use std::sync::Arc;

const WRITER: &str = "okf.v1.OkfWriterService";
const READER: &str = "okf.v1.OkfReaderService";

struct Okf {
    // Keeps the storage folder alive; it is deleted when the test ends.
    _dir: tempfile::TempDir,
    writer: NatsRpcClient,
    reader: NatsRpcClient,
    writer_state: Arc<dyn sdk::state::IStateStore<OkfWriterState>>,
}

/// Mounts a writer and a reader over one temp folder, the way two host processes share `--config root`.
async fn start(env: &mut TestEnv) -> anyhow::Result<Okf> {
    let dir = tempfile::tempdir()?;
    let inst = env.instance_name("docs");

    let writer_state = env.state_store::<OkfWriterState>("okf_writer", &inst);
    let writer = OkfWriterHandler::new(Arc::new(FileStore::new(dir.path())), writer_state.clone());
    env.mount_endpoint("okf_writer", &inst, Arc::new(OkfWriterEndpoint(Arc::new(writer)))).await?;

    let reader_state = env.state_store::<OkfReaderState>("okf_reader", &inst);
    let reader = OkfReaderHandler::new(Arc::new(FileStore::new(dir.path())), reader_state);
    env.mount_endpoint("okf_reader", &inst, Arc::new(OkfReaderEndpoint(Arc::new(reader)))).await?;

    Ok(Okf { _dir: dir, writer: env.okf_writer_client(&inst), reader: env.okf_reader_client(&inst), writer_state })
}

fn document(stream: &str, text: &str) -> OkfDocument {
    OkfDocument {
        stream: stream.into(),
        correlation_id: "run-1".into(),
        source: "e2e/test".into(),
        kind: "note".into(),
        content_type: "text/plain".into(),
        data: text.as_bytes().to_vec(),
        ..Default::default()
    }
}

#[tokio::test]
async fn appended_documents_are_read_back_in_server_time_order() -> anyhow::Result<()> {
    let mut env = TestEnv::connect().await?;
    let okf = start(&mut env).await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let req = OkfAppendRequest {
        documents: vec![document("notes", "first"), document("frames", "second"), document("notes", "third")],
    };
    let appended: OkfAppendResponse = okf.writer.call(WRITER, "Append", &req, &ProgressSender::new(tx)).await?;
    assert_eq!(appended.refs.len(), 3);
    assert!(rx.try_recv()?.contains("appending 3 documents"));

    let noop = ProgressSender::noop();
    let get_req = OkfGetRequest { id: appended.refs[1].id.clone() };
    let got: OkfGetResponse = okf.reader.call(READER, "Get", &get_req, &noop).await?;
    assert_eq!(got.document.expect("document").data, b"second");

    let range_req = OkfRangeRequest { from_micros: appended.refs[0].server_time_micros, ..Default::default() };
    let range: OkfRangeResponse = okf.reader.call(READER, "Range", &range_req, &noop).await?;
    let texts: Vec<&[u8]> = range.documents.iter().map(|d| d.data.as_slice()).collect();
    assert_eq!(texts, vec![b"first".as_slice(), b"second", b"third"]);
    assert_eq!(range.next_from_micros, 0);

    let tail_req = OkfTailRequest { stream: "notes".into(), limit: 1 };
    let tail: OkfTailResponse = okf.reader.call(READER, "Tail", &tail_req, &noop).await?;
    assert_eq!(tail.documents[0].id, appended.refs[2].id);

    let state = okf.writer_state.get().await?;
    assert_eq!(state.documents_appended, 3);
    assert_eq!(state.last_id, appended.refs[2].id);
    Ok(())
}

#[tokio::test]
async fn error_codes_cross_nats() -> anyhow::Result<()> {
    let mut env = TestEnv::connect().await?;
    let okf = start(&mut env).await?;
    let noop = ProgressSender::noop();

    let unknown = OkfGetRequest { id: "notes/2026-11-11/103045.123456".into() };
    let err = okf.reader.call::<_, OkfGetResponse>(READER, "Get", &unknown, &noop).await.unwrap_err();
    assert_eq!(err.code, "not_found");

    let bad = OkfAppendRequest { documents: vec![document("../escape", "x")] };
    let err = okf.writer.call::<_, OkfAppendResponse>(WRITER, "Append", &bad, &noop).await.unwrap_err();
    assert_eq!(err.code, "invalid_argument");
    Ok(())
}
