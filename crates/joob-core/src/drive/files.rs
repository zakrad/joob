use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(rename = "mimeType", default)]
    pub mime_type: String,
}

#[derive(Debug, Deserialize)]
pub struct FileListResponse {
    pub files: Vec<FileInfo>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FileMetadata {
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parents: Option<Vec<String>>,
}
