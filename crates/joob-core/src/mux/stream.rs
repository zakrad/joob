use bytes::Bytes;
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum StreamError {
    #[error("stream closed")]
    Closed,
}

/// A handle to a multiplexed stream.
/// Provides async read/write over a channel-based interface.
pub struct StreamHandle {
    pub stream_id: u32,
    pub target: String,
    tx: mpsc::Sender<Bytes>,
    rx: mpsc::Receiver<Bytes>,
}

impl StreamHandle {
    pub fn new(
        stream_id: u32,
        target: String,
        tx: mpsc::Sender<Bytes>,
        rx: mpsc::Receiver<Bytes>,
    ) -> Self {
        Self {
            stream_id,
            target,
            tx,
            rx,
        }
    }

    /// Send data to this stream (will be packed into DATA frames).
    pub async fn send(&self, data: Bytes) -> Result<(), StreamError> {
        self.tx.send(data).await.map_err(|_| StreamError::Closed)
    }

    /// Receive data from this stream.
    pub async fn recv(&mut self) -> Option<Bytes> {
        self.rx.recv().await
    }

    /// Clone the sender half for use in bridge tasks.
    pub fn tx_clone(&self) -> mpsc::Sender<Bytes> {
        self.tx.clone()
    }
}

/// Internal stream state tracked by the mux.
#[allow(dead_code)]
pub(crate) struct StreamState {
    pub stream_id: u32,
    pub target: String,
    /// Channel to send data TO this stream's consumer (proxy/dialer)
    pub to_consumer: mpsc::Sender<Bytes>,
    /// Channel to receive data FROM this stream's producer (proxy/dialer)
    pub from_producer: mpsc::Receiver<Bytes>,
    pub closed: bool,
}
