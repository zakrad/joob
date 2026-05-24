use crate::drive::client::{DriveClient, DriveError};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

pub struct Downloader {
    drive: Arc<DriveClient>,
    folder_id: String,
    prefix: String,
    state: Mutex<DownloadState>,
}

struct DownloadState {
    seq: u32,
    offset: u64,
    current_file_id: Option<String>,
}

impl Downloader {
    pub fn new(drive: Arc<DriveClient>, folder_id: String, prefix: String) -> Self {
        Self {
            drive,
            folder_id,
            prefix,
            state: Mutex::new(DownloadState {
                seq: 0,
                offset: 0,
                current_file_id: None,
            }),
        }
    }

    /// Try to read new data. Returns new bytes or `None` if no new data is available.
    pub async fn read(&self) -> Result<Option<Vec<u8>>, DriveError> {
        let mut state = self.state.lock().await;

        // If we don't have a current file, try to find one
        if state.current_file_id.is_none() {
            let file_name = format!("{}_{:06}.bin", self.prefix, state.seq);
            let files = self
                .drive
                .list_files(&self.folder_id, Some(&file_name))
                .await?;

            if let Some(file) = files.into_iter().find(|f| f.name == file_name) {
                debug!(prefix = %self.prefix, seq = state.seq, "found file: {}", file.name);
                state.current_file_id = Some(file.id);
                state.offset = 0;
            } else {
                return Ok(None); // No file yet
            }
        }

        let file_id = state.current_file_id.as_ref().unwrap().clone();

        // Try range read from current offset
        match self.drive.get_file_range(&file_id, state.offset).await {
            Ok(data) if data.is_empty() => Ok(None),
            Ok(data) => {
                state.offset += data.len() as u64;
                Ok(Some(data))
            }
            Err(DriveError::NoNewData) => {
                // Check if next file exists (rotation happened)
                let next_name = format!("{}_{:06}.bin", self.prefix, state.seq + 1);
                let files = self
                    .drive
                    .list_files(&self.folder_id, Some(&next_name))
                    .await?;

                if let Some(file) = files.into_iter().find(|f| f.name == next_name) {
                    debug!(prefix = %self.prefix, seq = state.seq + 1, "switching to next file");
                    state.seq += 1;
                    state.offset = 0;
                    let new_file_id = file.id;
                    state.current_file_id = Some(new_file_id.clone());
                    // Read from the beginning of the new file
                    match self.drive.get_file_range(&new_file_id, 0).await {
                        Ok(data) if data.is_empty() => Ok(None),
                        Ok(data) => {
                            state.offset = data.len() as u64;
                            Ok(Some(data))
                        }
                        Err(DriveError::NoNewData) => Ok(None),
                        Err(e) => Err(e),
                    }
                } else {
                    Ok(None) // No new data and no next file
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Get current sequence number.
    pub async fn current_seq(&self) -> u32 {
        self.state.lock().await.seq
    }
}
