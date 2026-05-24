use crate::drive::client::{DriveClient, DriveError};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

const DEFAULT_MAX_FILE_SIZE: usize = 10 * 1024 * 1024; // 10MB

pub struct Uploader {
    drive: Arc<DriveClient>,
    folder_id: String,
    prefix: String,
    seq: Mutex<u32>,
    current_file_id: Mutex<Option<String>>,
    current_data: Mutex<Vec<u8>>,
    max_file_size: usize,
}

impl Uploader {
    pub fn new(drive: Arc<DriveClient>, folder_id: String, prefix: String) -> Self {
        Self {
            drive,
            folder_id,
            prefix,
            seq: Mutex::new(0),
            current_file_id: Mutex::new(None),
            current_data: Mutex::new(Vec::new()),
            max_file_size: DEFAULT_MAX_FILE_SIZE,
        }
    }

    pub fn with_max_file_size(mut self, size: usize) -> Self {
        self.max_file_size = size;
        self
    }

    /// Write data to the current file. Handles rotation automatically when file
    /// size would exceed `max_file_size`.
    pub async fn write(&self, data: &[u8]) -> Result<(), DriveError> {
        let mut current_data = self.current_data.lock().await;
        let mut current_file_id = self.current_file_id.lock().await;
        let mut seq = self.seq.lock().await;

        // Check if we need to rotate (current file would exceed max size)
        if current_data.len() + data.len() > self.max_file_size && !current_data.is_empty() {
            debug!(prefix = %self.prefix, seq = *seq, size = current_data.len(), "rotating file");
            *current_file_id = None;
            current_data.clear();
            *seq += 1;
        }

        // Append new data to buffer
        current_data.extend_from_slice(data);

        let file_name = format!("{}_{:06}.bin", self.prefix, *seq);

        match current_file_id.as_ref() {
            Some(file_id) => {
                // Update existing file with all accumulated data
                self.drive.update_file(file_id, &current_data).await?;
            }
            None => {
                // Create new file
                let file_id = self
                    .drive
                    .create_file(&file_name, &self.folder_id, &current_data)
                    .await?;
                *current_file_id = Some(file_id);
            }
        }

        Ok(())
    }

    /// Force rotation: finalize current file and start a new one.
    pub async fn rotate(&self) -> Result<(), DriveError> {
        let mut current_file_id = self.current_file_id.lock().await;
        let mut current_data = self.current_data.lock().await;
        let mut seq = self.seq.lock().await;

        if current_file_id.is_some() {
            *seq += 1;
            *current_file_id = None;
            current_data.clear();
        }
        Ok(())
    }

    /// Get current sequence number.
    pub async fn current_seq(&self) -> u32 {
        *self.seq.lock().await
    }

    /// Get the file name for a given sequence number.
    pub fn file_name_for_seq(&self, seq: u32) -> String {
        format!("{}_{:06}.bin", self.prefix, seq)
    }
}
