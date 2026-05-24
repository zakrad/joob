use crate::mux::frame::{Frame, FrameType};
use crate::mux::stream::{StreamError, StreamHandle, StreamState};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::{mpsc, Mutex};
use tracing::debug;

const STREAM_CHANNEL_SIZE: usize = 256;

pub struct Mux {
    streams: Mutex<HashMap<u32, StreamState>>,
    next_stream_id: AtomicU32,
    /// Outbound frames waiting to be sent
    outbound_tx: mpsc::Sender<Frame>,
    outbound_rx: Mutex<mpsc::Receiver<Frame>>,
    /// Callback for when exit side receives an OPEN frame
    on_stream_open: Mutex<Option<Box<dyn Fn(StreamHandle) + Send + Sync>>>,
}

impl Mux {
    pub fn new() -> Self {
        let (outbound_tx, outbound_rx) = mpsc::channel(4096);
        Self {
            streams: Mutex::new(HashMap::new()),
            next_stream_id: AtomicU32::new(1),
            outbound_tx,
            outbound_rx: Mutex::new(outbound_rx),
            on_stream_open: Mutex::new(None),
        }
    }

    /// Set callback for incoming stream opens (exit side).
    pub async fn set_on_stream_open<F>(&self, callback: F)
    where
        F: Fn(StreamHandle) + Send + Sync + 'static,
    {
        *self.on_stream_open.lock().await = Some(Box::new(callback));
    }

    /// Open a new stream (client side). Returns a StreamHandle for bidirectional I/O.
    pub async fn open_stream(&self, target: &str) -> Result<StreamHandle, StreamError> {
        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);

        // Create channels:
        //   to_consumer: mux → consumer (incoming data from remote)
        //   from_producer: consumer → mux (outgoing data to remote)
        let (to_consumer_tx, to_consumer_rx) = mpsc::channel(STREAM_CHANNEL_SIZE);
        let (from_producer_tx, from_producer_rx) = mpsc::channel(STREAM_CHANNEL_SIZE);

        let handle = StreamHandle::new(
            stream_id,
            target.to_string(),
            from_producer_tx,
            to_consumer_rx,
        );

        let state = StreamState {
            stream_id,
            target: target.to_string(),
            to_consumer: to_consumer_tx,
            from_producer: from_producer_rx,
            closed: false,
        };

        self.streams.lock().await.insert(stream_id, state);

        // Send OPEN frame
        let open_frame = Frame {
            stream_id,
            frame_type: FrameType::Open,
            payload: target.as_bytes().to_vec(),
        };
        let _ = self.outbound_tx.send(open_frame).await;

        debug!(stream_id, target, "opened stream");
        Ok(handle)
    }

    /// Dispatch an incoming frame to the correct stream.
    pub async fn dispatch(&self, frame: Frame) {
        match frame.frame_type {
            FrameType::Open => {
                // Remote side opened a stream — create local state and notify callback
                let target = String::from_utf8_lossy(&frame.payload).to_string();
                let stream_id = frame.stream_id;

                let (to_consumer_tx, to_consumer_rx) = mpsc::channel(STREAM_CHANNEL_SIZE);
                let (from_producer_tx, from_producer_rx) = mpsc::channel(STREAM_CHANNEL_SIZE);

                let handle = StreamHandle::new(
                    stream_id,
                    target.clone(),
                    from_producer_tx,
                    to_consumer_rx,
                );

                let state = StreamState {
                    stream_id,
                    target: target.clone(),
                    to_consumer: to_consumer_tx,
                    from_producer: from_producer_rx,
                    closed: false,
                };

                self.streams.lock().await.insert(stream_id, state);

                // Update next_stream_id to avoid collisions
                let _ = self
                    .next_stream_id
                    .fetch_max(stream_id + 1, Ordering::SeqCst);

                debug!(stream_id, target, "remote opened stream");

                if let Some(callback) = self.on_stream_open.lock().await.as_ref() {
                    callback(handle);
                }
            }
            FrameType::Data => {
                let streams = self.streams.lock().await;
                if let Some(state) = streams.get(&frame.stream_id) {
                    if !state.closed {
                        let _ = state.to_consumer.send(Bytes::from(frame.payload)).await;
                    }
                }
            }
            FrameType::Fin | FrameType::Rst => {
                let mut streams = self.streams.lock().await;
                if let Some(state) = streams.get_mut(&frame.stream_id) {
                    state.closed = true;
                    debug!(stream_id = frame.stream_id, "stream closed");
                }
            }
            FrameType::Ping => {
                let pong = Frame {
                    stream_id: 0,
                    frame_type: FrameType::Pong,
                    payload: frame.payload,
                };
                let _ = self.outbound_tx.send(pong).await;
            }
            FrameType::Pong => {
                // Could track latency here
            }
        }
    }

    /// Collect outbound frames from the mux (frames waiting to be sent).
    pub async fn collect_outbound(&self) -> Vec<Frame> {
        let mut frames = Vec::new();
        let mut rx = self.outbound_rx.lock().await;

        // Drain available frames without blocking
        while let Ok(frame) = rx.try_recv() {
            frames.push(frame);
        }

        // Also collect DATA frames from streams that have outgoing data
        let mut streams = self.streams.lock().await;
        let mut closed_ids = Vec::new();

        for (id, state) in streams.iter_mut() {
            if state.closed {
                closed_ids.push(*id);
                continue;
            }

            // Drain data from producer channel
            while let Ok(data) = state.from_producer.try_recv() {
                frames.push(Frame {
                    stream_id: state.stream_id,
                    frame_type: FrameType::Data,
                    payload: data.to_vec(),
                });
            }
        }

        // Clean up closed streams
        for id in closed_ids {
            streams.remove(&id);
        }

        frames
    }

    /// Send a FIN frame for a stream.
    pub async fn close_stream(&self, stream_id: u32) {
        let fin = Frame {
            stream_id,
            frame_type: FrameType::Fin,
            payload: Vec::new(),
        };
        let _ = self.outbound_tx.send(fin).await;

        let mut streams = self.streams.lock().await;
        if let Some(state) = streams.get_mut(&stream_id) {
            state.closed = true;
        }
    }

    /// Get number of active streams.
    pub async fn active_stream_count(&self) -> usize {
        self.streams.lock().await.len()
    }
}

impl Default for Mux {
    fn default() -> Self {
        Self::new()
    }
}
