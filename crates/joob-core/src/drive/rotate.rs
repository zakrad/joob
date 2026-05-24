use crate::drive::client::{DriveClient, DriveError};
use std::sync::Arc;
use tracing::{debug, warn};

pub struct FileRotator {
    drive: Arc<DriveClient>,
    folder_id: String,
}

impl FileRotator {
    pub fn new(drive: Arc<DriveClient>, folder_id: String) -> Self {
        Self { drive, folder_id }
    }

    /// Clean up old files that are behind both the reader and writer.
    /// Keeps files with sequence >= `min_keep_seq`.
    pub async fn cleanup(&self, prefix: &str, min_keep_seq: u32) -> Result<u32, DriveError> {
        let files = self.drive.list_files(&self.folder_id, Some(prefix)).await?;
        let mut deleted = 0u32;

        for file in files {
            if let Some(seq) = Self::parse_seq(&file.name, prefix) {
                if seq < min_keep_seq {
                    debug!(name = %file.name, seq, "deleting old file");
                    if let Err(e) = self.drive.delete_file(&file.id).await {
                        warn!(name = %file.name, error = %e, "failed to delete old file");
                    } else {
                        deleted += 1;
                    }
                }
            }
        }

        Ok(deleted)
    }

    /// Delete ALL files with the given prefix.
    pub async fn cleanup_all(&self, prefix: &str) -> Result<u32, DriveError> {
        let files = self.drive.list_files(&self.folder_id, Some(prefix)).await?;
        let mut deleted = 0u32;

        for file in files {
            if let Err(e) = self.drive.delete_file(&file.id).await {
                warn!(name = %file.name, error = %e, "failed to delete file");
            } else {
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    /// Parse sequence number from file name like "up_000042.bin" → Some(42)
    fn parse_seq(name: &str, prefix: &str) -> Option<u32> {
        let expected_prefix = format!("{}_", prefix);
        if !name.starts_with(&expected_prefix) || !name.ends_with(".bin") {
            return None;
        }
        let seq_str = &name[expected_prefix.len()..name.len() - 4]; // strip prefix_ and .bin
        seq_str.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_seq() {
        assert_eq!(FileRotator::parse_seq("up_000000.bin", "up"), Some(0));
        assert_eq!(FileRotator::parse_seq("up_000042.bin", "up"), Some(42));
        assert_eq!(FileRotator::parse_seq("dn_001234.bin", "dn"), Some(1234));
        assert_eq!(FileRotator::parse_seq("up_000042.bin", "dn"), None);
        assert_eq!(FileRotator::parse_seq("random.txt", "up"), None);
        assert_eq!(FileRotator::parse_seq("up_.bin", "up"), None);
    }
}
