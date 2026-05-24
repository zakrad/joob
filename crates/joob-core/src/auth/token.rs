use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Stored OAuth token data containing access token, refresh token, and expiry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

impl TokenData {
    /// Check if the token is expired or will expire within the given margin.
    pub fn is_expired(&self, margin_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now + margin_secs >= self.expires_at
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no token available — run setup first")]
    NoToken,
}

/// Thread-safe token store with file persistence.
///
/// Tokens are held in memory behind an `RwLock` and persisted to a JSON file
/// so they survive process restarts.
pub struct TokenStore {
    path: PathBuf,
    data: Arc<RwLock<Option<TokenData>>>,
}

impl TokenStore {
    /// Create a new token store. Loads existing token from file if present.
    pub fn new(path: PathBuf) -> Self {
        let data = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => serde_json::from_str(&contents).ok(),
                Err(_) => None,
            }
        } else {
            None
        };

        Self {
            path,
            data: Arc::new(RwLock::new(data)),
        }
    }

    /// Get the current token data. Returns `None` if no token is stored.
    pub async fn get(&self) -> Option<TokenData> {
        self.data.read().await.clone()
    }

    /// Get a valid access token string, or error if no token is available.
    pub async fn access_token(&self) -> Result<String, TokenError> {
        let data = self.data.read().await;
        match data.as_ref() {
            Some(t) => Ok(t.access_token.clone()),
            None => Err(TokenError::NoToken),
        }
    }

    /// Store new token data (in memory + persist to file).
    pub async fn store(&self, token: TokenData) -> Result<(), TokenError> {
        let json = serde_json::to_string_pretty(&token)?;

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&self.path, json).await?;
        *self.data.write().await = Some(token);
        Ok(())
    }

    /// Check if token needs refresh (expired or within margin).
    pub async fn needs_refresh(&self, margin_secs: u64) -> bool {
        match self.data.read().await.as_ref() {
            Some(t) => t.is_expired(margin_secs),
            None => true,
        }
    }

    /// Get the refresh token string if available.
    pub async fn refresh_token(&self) -> Option<String> {
        self.data
            .read()
            .await
            .as_ref()
            .map(|t| t.refresh_token.clone())
    }

    /// Delete stored token (memory + file).
    pub async fn clear(&self) -> Result<(), TokenError> {
        *self.data.write().await = None;
        if self.path.exists() {
            tokio::fs::remove_file(&self.path).await?;
        }
        Ok(())
    }
}
