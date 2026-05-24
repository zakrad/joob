use crate::auth::{DeviceCodeFlow, OAuthConfig, TokenStore};
use crate::drive::files::{FileInfo, FileListResponse};
use reqwest::Client;
use std::sync::Arc;
use tracing::warn;

const DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";

/// Build API/upload base URLs for a Cloudflare Worker frontend.
/// The Worker is expected to map `/drive/v3/...` → `googleapis.com/drive/v3/...`
/// and `/upload/drive/v3/...` → `googleapis.com/upload/drive/v3/...`.
fn frontend_bases(frontend_base_url: &str) -> (String, String) {
    let base = frontend_base_url.trim_end_matches('/');
    (
        format!("{}/drive/v3", base),
        format!("{}/upload/drive/v3", base),
    )
}

#[derive(Debug, thiserror::Error)]
pub enum DriveError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("token error: {0}")]
    Token(String),
    #[error("rate limited — retry after backoff")]
    RateLimited,
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("no new data available")]
    NoNewData,
}

pub struct DriveClient {
    http: Client,
    token_store: Arc<TokenStore>,
    oauth_config: OAuthConfig,
    api_base: String,
    upload_base: String,
    /// Optional Worker frontend URL used to rewrite `oauth2.googleapis.com`
    /// when refreshing tokens from inside this client.
    frontend_base: Option<String>,
}

impl DriveClient {
    pub fn new(http: Client, token_store: Arc<TokenStore>, oauth_config: OAuthConfig) -> Self {
        Self {
            http,
            token_store,
            oauth_config,
            api_base: DRIVE_API_BASE.to_string(),
            upload_base: DRIVE_UPLOAD_BASE.to_string(),
            frontend_base: None,
        }
    }

    /// Construct a `DriveClient` that routes all Drive API calls through a
    /// Cloudflare Worker frontend instead of `www.googleapis.com` directly.
    pub fn with_frontend(
        http: Client,
        token_store: Arc<TokenStore>,
        oauth_config: OAuthConfig,
        frontend_base_url: &str,
    ) -> Self {
        let (api_base, upload_base) = frontend_bases(frontend_base_url);
        Self {
            http,
            token_store,
            oauth_config,
            api_base,
            upload_base,
            frontend_base: Some(frontend_base_url.trim_end_matches('/').to_string()),
        }
    }

    /// Get a valid access token, refreshing if necessary.
    async fn access_token(&self) -> Result<String, DriveError> {
        // Check if token needs refresh (within 5 min of expiry)
        if self.token_store.needs_refresh(300).await {
            if let Some(refresh_token) = self.token_store.refresh_token().await {
                let flow = match &self.frontend_base {
                    Some(base) => DeviceCodeFlow::with_frontend(
                        self.oauth_config.clone(),
                        self.http.clone(),
                        base,
                    ),
                    None => DeviceCodeFlow::new(self.oauth_config.clone(), self.http.clone()),
                };
                match flow.refresh_token(&refresh_token).await {
                    Ok(new_token) => {
                        self.token_store
                            .store(new_token)
                            .await
                            .map_err(|e| DriveError::Token(e.to_string()))?;
                    }
                    Err(e) => {
                        warn!("Token refresh failed: {}", e);
                        return Err(DriveError::Token(e.to_string()));
                    }
                }
            }
        }

        self.token_store
            .access_token()
            .await
            .map_err(|e| DriveError::Token(e.to_string()))
    }

    /// Check HTTP response status and return the response on success, or an error.
    async fn ensure_success(resp: reqwest::Response) -> Result<reqwest::Response, DriveError> {
        let status = resp.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(resp);
        }

        match status {
            429 => Err(DriveError::RateLimited),
            404 => Err(DriveError::NotFound("resource not found".into())),
            _ => {
                let body = resp.text().await.unwrap_or_default();
                Err(DriveError::Api { status, body })
            }
        }
    }

    /// Create a folder in Drive. Returns folder ID.
    pub async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<String, DriveError> {
        let token = self.access_token().await?;

        let mut metadata = serde_json::json!({
            "name": name,
            "mimeType": "application/vnd.google-apps.folder"
        });

        if let Some(parent) = parent_id {
            metadata["parents"] = serde_json::json!([parent]);
        }

        let resp = self
            .http
            .post(&format!("{}/files", self.api_base))
            .bearer_auth(&token)
            .json(&metadata)
            .send()
            .await?;

        let resp = Self::ensure_success(resp).await?;

        let body: serde_json::Value = resp.json().await?;
        Ok(body["id"].as_str().unwrap_or_default().to_string())
    }

    /// Create a file with content using multipart upload. Returns file ID.
    pub async fn create_file(
        &self,
        name: &str,
        folder_id: &str,
        data: &[u8],
    ) -> Result<String, DriveError> {
        let token = self.access_token().await?;

        let metadata = serde_json::json!({
            "name": name,
            "parents": [folder_id],
            "mimeType": "application/octet-stream"
        });

        // Multipart upload: metadata + content
        let boundary = "joob_boundary_12345";
        let mut body = Vec::new();

        // Part 1: metadata
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
        body.extend_from_slice(serde_json::to_string(&metadata).unwrap().as_bytes());
        body.extend_from_slice(b"\r\n");

        // Part 2: file content
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(format!("--{}--", boundary).as_bytes());

        let resp = self
            .http
            .post(&format!("{}/files?uploadType=multipart", self.upload_base))
            .bearer_auth(&token)
            .header(
                "Content-Type",
                format!("multipart/related; boundary={}", boundary),
            )
            .body(body)
            .send()
            .await?;

        let resp = Self::ensure_success(resp).await?;

        let json: serde_json::Value = resp.json().await?;
        Ok(json["id"].as_str().unwrap_or_default().to_string())
    }

    /// Update (overwrite) file content.
    pub async fn update_file(&self, file_id: &str, data: &[u8]) -> Result<(), DriveError> {
        let token = self.access_token().await?;

        let resp = self
            .http
            .patch(&format!(
                "{}/files/{}?uploadType=media",
                self.upload_base, file_id
            ))
            .bearer_auth(&token)
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await?;

        Self::ensure_success(resp).await?;
        Ok(())
    }

    /// Get full file content.
    pub async fn get_file_content(&self, file_id: &str) -> Result<Vec<u8>, DriveError> {
        let token = self.access_token().await?;

        let resp = self
            .http
            .get(&format!("{}/files/{}?alt=media", self.api_base, file_id))
            .bearer_auth(&token)
            .send()
            .await?;

        let resp = Self::ensure_success(resp).await?;
        Ok(resp.bytes().await?.to_vec())
    }

    /// Get file content from a specific byte offset (Range GET).
    /// Returns the new bytes, or `DriveError::NoNewData` if no new data is available.
    pub async fn get_file_range(&self, file_id: &str, offset: u64) -> Result<Vec<u8>, DriveError> {
        let token = self.access_token().await?;

        let resp = self
            .http
            .get(&format!("{}/files/{}?alt=media", self.api_base, file_id))
            .bearer_auth(&token)
            .header("Range", format!("bytes={}-", offset))
            .send()
            .await?;

        let status = resp.status().as_u16();

        // 416 Range Not Satisfiable = no new data
        if status == 416 {
            return Err(DriveError::NoNewData);
        }

        // 206 Partial Content or 200 OK = got data
        if status == 206 || status == 200 {
            return Ok(resp.bytes().await?.to_vec());
        }

        // Other errors
        let resp = Self::ensure_success(resp).await?;
        Ok(resp.bytes().await?.to_vec())
    }

    /// Get file size via metadata.
    pub async fn get_file_size(&self, file_id: &str) -> Result<u64, DriveError> {
        let token = self.access_token().await?;

        let resp = self
            .http
            .get(&format!(
                "{}/files/{}?fields=size",
                self.api_base, file_id
            ))
            .bearer_auth(&token)
            .send()
            .await?;

        let resp = Self::ensure_success(resp).await?;

        let body: serde_json::Value = resp.json().await?;
        let size = body["size"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        Ok(size)
    }

    /// List files in a folder with optional name prefix filter.
    pub async fn list_files(
        &self,
        folder_id: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<FileInfo>, DriveError> {
        let token = self.access_token().await?;

        let mut query = format!("'{}' in parents and trashed = false", folder_id);
        if let Some(p) = prefix {
            query.push_str(&format!(" and name contains '{}'", p));
        }

        let resp = self
            .http
            .get(&format!("{}/files", self.api_base))
            .bearer_auth(&token)
            .query(&[
                ("q", query.as_str()),
                ("fields", "files(id,name,size,mimeType),nextPageToken"),
                ("orderBy", "name"),
                ("pageSize", "1000"),
            ])
            .send()
            .await?;

        let resp = Self::ensure_success(resp).await?;

        let list: FileListResponse = resp.json().await?;
        Ok(list.files)
    }

    /// Delete a file.
    pub async fn delete_file(&self, file_id: &str) -> Result<(), DriveError> {
        let token = self.access_token().await?;

        let resp = self
            .http
            .delete(&format!("{}/files/{}", self.api_base, file_id))
            .bearer_auth(&token)
            .send()
            .await?;

        // 204 No Content = success, 404 = already deleted (ok)
        let status = resp.status().as_u16();
        if status == 204 || status == 404 {
            return Ok(());
        }

        Self::ensure_success(resp).await?;
        Ok(())
    }
}
