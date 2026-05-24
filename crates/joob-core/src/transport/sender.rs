use crate::crypto::Cipher;
use crate::drive::{QuotaTracker, Uploader};
use crate::mux::{FrameBatch, Mux};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{debug, error};

const FLUSH_INTERVAL_MS: u64 = 100;

pub struct Sender {
    mux: Arc<Mux>,
    uploader: Arc<Uploader>,
    cipher: Arc<Cipher>,
    quota: Arc<QuotaTracker>,
    flush_interval: Duration,
}

impl Sender {
    pub fn new(
        mux: Arc<Mux>,
        uploader: Arc<Uploader>,
        cipher: Arc<Cipher>,
        quota: Arc<QuotaTracker>,
    ) -> Self {
        Self {
            mux,
            uploader,
            cipher,
            quota,
            flush_interval: Duration::from_millis(FLUSH_INTERVAL_MS),
        }
    }

    /// Run the sender loop. Collects frames from mux, batches, encrypts, uploads.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        loop {
            // Wait for flush interval
            time::sleep(self.flush_interval).await;

            // Collect outbound frames
            let frames = self.mux.collect_outbound().await;
            if frames.is_empty() {
                continue;
            }

            // Encode frames into binary batch
            let batch_data = FrameBatch::encode(&frames);
            if batch_data.is_empty() {
                continue;
            }

            // Encrypt
            let encrypted = self.cipher.seal(&batch_data);

            // Throttle if needed
            self.quota.throttle().await;

            // Upload to Drive
            if let Err(e) = self.uploader.write(&encrypted).await {
                error!(error = %e, "failed to upload data");
                // Don't crash — retry next cycle
                time::sleep(Duration::from_secs(1)).await;
            } else {
                debug!(frames = frames.len(), bytes = encrypted.len(), "sent batch");
            }
        }
    }
}
