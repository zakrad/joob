use crate::crypto::Cipher;
use crate::drive::{Downloader, QuotaTracker, Uploader};
use crate::mux::Mux;
use crate::transport::receiver::Receiver;
use crate::transport::sender::Sender;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::error;

pub struct Pipe {
    sender_handle: Option<JoinHandle<()>>,
    receiver_handle: Option<JoinHandle<()>>,
}

impl Pipe {
    /// Start the bidirectional pipe.
    pub fn start(
        mux: Arc<Mux>,
        uploader: Arc<Uploader>,
        downloader: Arc<Downloader>,
        send_cipher: Arc<Cipher>,
        recv_cipher: Arc<Cipher>,
        quota: Arc<QuotaTracker>,
    ) -> Self {
        let sender = Sender::new(
            Arc::clone(&mux),
            uploader,
            send_cipher,
            Arc::clone(&quota),
        );

        let receiver = Receiver::new(mux, downloader, recv_cipher, quota);

        let sender_handle = tokio::spawn(async move {
            if let Err(e) = sender.run().await {
                error!(error = %e, "sender stopped");
            }
        });

        let receiver_handle = tokio::spawn(async move {
            if let Err(e) = receiver.run().await {
                error!(error = %e, "receiver stopped");
            }
        });

        Self {
            sender_handle: Some(sender_handle),
            receiver_handle: Some(receiver_handle),
        }
    }

    /// Stop the pipe.
    pub async fn stop(&mut self) {
        if let Some(h) = self.sender_handle.take() {
            h.abort();
        }
        if let Some(h) = self.receiver_handle.take() {
            h.abort();
        }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        if let Some(h) = self.sender_handle.take() {
            h.abort();
        }
        if let Some(h) = self.receiver_handle.take() {
            h.abort();
        }
    }
}
