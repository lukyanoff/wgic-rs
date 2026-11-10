//! Server streaming as a plain async function: `stream_from(|tx| async move { tx.send(a); tx.send(b); })`.
//!
//! ≈ C# `async IAsyncEnumerable<T>` with `yield return`. The function runs *inside* the response stream (no
//! background task), so when the client goes away or its deadline passes, the stream is dropped and the
//! function stops at its next `.await`: child processes and HTTP requests it owns are dropped with it.

use std::future::Future;

use connectrpc::{ConnectError, ServiceStream};
use futures::channel::mpsc;
use futures::{StreamExt, stream};

/// Writes messages into a response stream (≈ C# `ChannelWriter<T>`). Cheap to clone: give a copy to each
/// parallel task that reports progress.
pub struct StreamTx<T> {
    tx: mpsc::UnboundedSender<Result<T, ConnectError>>,
}

impl<T> Clone for StreamTx<T> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

impl<T> StreamTx<T> {
    /// Send one message to the client.
    pub fn send(&self, message: T) {
        let _ = self.tx.unbounded_send(Ok(message)); // fails only when the client is gone: nothing to do
    }

    /// End the stream with `error` (send nothing after it).
    pub fn fail(&self, error: ConnectError) {
        let _ = self.tx.unbounded_send(Err(error));
    }
}

/// A response stream fed by `work`: every `tx.send(..)` becomes one message, and the stream ends when
/// `work` returns.
///
/// ```ignore
/// Response::ok(stream_from(|tx| async move {
///     tx.send(progress("started"));
///     match do_the_work().await {
///         Ok(result) => tx.send(result),
///         Err(error) => tx.fail(error),
///     }
/// }))
/// ```
pub fn stream_from<T, F>(work: impl FnOnce(StreamTx<T>) -> F) -> ServiceStream<T>
where
    T: Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded();
    let work = work(StreamTx { tx });
    // Two streams merged into one: `rx` yields the messages, `run` only drives `work` and yields nothing.
    // `rx` ends once `work` has finished and dropped its `StreamTx`.
    let run = stream::once(work).filter_map(|()| async { None });
    Box::pin(stream::select(rx, run))
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
