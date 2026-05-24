use crate::crypto::Cipher;
use crate::drive::{Downloader, QuotaTracker};
use crate::mux::{FrameBatch, Mux};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{debug, error, warn};

const POLL_INTERVAL_MS: u64 = 150;

pub struct Receiver {
    mux: Arc<Mux>,
    downloader: Arc<Downloader>,
    cipher: Arc<Cipher>,
    quota: Arc<QuotaTracker>,
    poll_interval: Duration,
}

impl Receiver {
    pub fn new(
        mux: Arc<Mux>,
        downloader: Arc<Downloader>,
        cipher: Arc<Cipher>,
        quota: Arc<QuotaTracker>,
    ) -> Self {
        Self {
            mux,
            downloader,
            cipher,
            quota,
            poll_interval: Duration::from_millis(POLL_INTERVAL_MS),
        }
    }

    /// Run the receiver loop. Polls Drive for new data, decrypts, dispatches to mux.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        loop {
            // Throttle if needed
            self.quota.throttle().await;

            // Poll for new data
            match self.downloader.read().await {
                Ok(Some(encrypted)) => {
                    // Decrypt
                    match self.cipher.open(&encrypted) {
                        Ok(plaintext) => {
                            // Decode frame batch
                            match FrameBatch::decode(&plaintext) {
                                Ok(frames) => {
                                    debug!(
                                        frames = frames.len(),
                                        bytes = encrypted.len(),
                                        "received batch"
                                    );
                                    for frame in frames {
                                        self.mux.dispatch(frame).await;
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "failed to decode frame batch");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "failed to decrypt data");
                        }
                    }
                }
                Ok(None) => {
                    // No new data — wait before polling again
                    time::sleep(self.poll_interval).await;
                }
                Err(e) => {
                    error!(error = %e, "failed to read from Drive");
                    time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}
