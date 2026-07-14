//! SSE helper: build an [axum](https://docs.rs/axum/latest/axum/) SSE
//! response that streams a `StreamEvent` channel.

use axum::response::sse::{Event, Sse};
use futures::stream::Stream;
use mewcode_protocol::StreamEvent;
use std::convert::Infallible;

/// Convert a [`tokio::sync::mpsc::Receiver<StreamEvent>`](https://docs.rs/tokio/latest/tokio/sync/mpsc/struct.Receiver.html)
/// into an [axum](https://docs.rs/axum/latest/axum/) SSE body.
pub fn from_channel(
    rx: tokio::sync::mpsc::Receiver<StreamEvent>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut rx = rx;
        while let Some(event) = rx.recv().await {
            let data = match serde_json::to_string(&event) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "failed to serialise StreamEvent");
                    continue;
                }
            };
            yield Ok(Event::default().data(data));
        }
    };
    Sse::new(stream)
}
