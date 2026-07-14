//! Streaming helpers.

use mewcode_protocol::StreamEvent;
use tokio::sync::mpsc;

/// Fan-out helper for streaming events to multiple subscribers.
#[derive(Debug, Default, Clone)]
pub struct StreamBroadcaster {
    inner: Vec<mpsc::Sender<StreamEvent>>,
}

impl StreamBroadcaster {
    /// Build a new broadcaster.
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe, returning a receiver.
    pub fn subscribe(&mut self) -> mpsc::Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel(64);
        self.inner.push(tx);
        rx
    }

    /// Send an event to every subscriber. Slow subscribers are dropped
    /// (their `send` returns an error, which we ignore).
    pub async fn broadcast(&self, event: StreamEvent) {
        for tx in &self.inner {
            let _ = tx.send(event.clone()).await;
        }
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.inner.iter().filter(|tx| !tx.is_closed()).count()
    }
}
