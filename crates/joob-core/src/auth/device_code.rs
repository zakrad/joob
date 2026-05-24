use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::info;

use crate::auth::token::TokenData;

const GOOGLE_DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DRIVE_FILE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

// Default OAuth client — users can bring their own
const DEFAULT_CLIENT_ID: &str = "PLACEHOLDER_CLIENT_ID";
const DEFAULT_CLIENT_SECRET: &str = "PLACEHOLDER_CLIENT_SECRET";

/// OAuth client credentials for the device-code flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            client_id: DEFAULT_CLIENT_ID.to_string(),
            client_secret: DEFAULT_CLIENT_SECRET.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TokenResponse {
    Success {
        access_token: String,
        refresh_token: String,
        expires_in: u64,
        #[allow(dead_code)]
        token_type: String,
    },
    Pending {
        error: String,
        #[allow(dead_code)]
        error_description: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("device code expired — user did not authorize in time")]
    DeviceCodeExpired,
    #[error("authorization denied by user")]
    AccessDenied,
    #[error("token refresh failed: {0}")]
    RefreshFailed(String),
    #[error("unexpected error: {0}")]
    Unexpected(String),
}

/// Implements the Google OAuth 2.0 device-code flow.
///
/// This flow is designed for devices that cannot open a browser directly.
/// The user is shown a URL and a code to enter on another device.
pub struct DeviceCodeFlow {
    config: OAuthConfig,
    http: reqwest::Client,
}

impl DeviceCodeFlow {
    pub fn new(config: OAuthConfig, http: reqwest::Client) -> Self {
        Self { config, http }
    }

    /// Start the device-code authorization flow.
    ///
    /// Prints a URL and code for the user to visit and authorize, then polls
    /// until the user completes authorization or the code expires.
    ///
    /// Returns `TokenData` with access_token, refresh_token, and expiry.
    pub async fn authorize(&self) -> Result<TokenData, AuthError> {
        // Step 1: Request device code
        let resp = self
            .http
            .post(GOOGLE_DEVICE_CODE_URL)
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("scope", DRIVE_FILE_SCOPE),
            ])
            .send()
            .await?
            .json::<DeviceCodeResponse>()
            .await?;

        println!("\n╔══════════════════════════════════════════╗");
        println!("║         GOOGLE AUTHORIZATION             ║");
        println!("╠══════════════════════════════════════════╣");
        println!("║                                          ║");
        println!("║  Visit: {:<31} ║", resp.verification_url);
        println!("║  Code:  {:<31} ║", resp.user_code);
        println!("║                                          ║");
        println!("╚══════════════════════════════════════════╝\n");

        // Step 2: Poll for authorization
        let interval = Duration::from_secs(resp.interval.max(5));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(resp.expires_in);

        loop {
            tokio::time::sleep(interval).await;

            if tokio::time::Instant::now() > deadline {
                return Err(AuthError::DeviceCodeExpired);
            }

            let token_resp = self
                .http
                .post(GOOGLE_TOKEN_URL)
                .form(&[
                    ("client_id", self.config.client_id.as_str()),
                    ("client_secret", self.config.client_secret.as_str()),
                    ("device_code", resp.device_code.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await?
                .json::<TokenResponse>()
                .await?;

            match token_resp {
                TokenResponse::Success {
                    access_token,
                    refresh_token,
                    expires_in,
                    ..
                } => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    info!("Authorization successful");
                    return Ok(TokenData {
                        access_token,
                        refresh_token,
                        expires_at: now + expires_in,
                    });
                }
                TokenResponse::Pending { error, .. } => match error.as_str() {
                    "authorization_pending" => {
                        continue;
                    }
                    "slow_down" => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                    "access_denied" => {
                        return Err(AuthError::AccessDenied);
                    }
                    _ => {
                        return Err(AuthError::Unexpected(error));
                    }
                },
            }
        }
    }

    /// Refresh an expired access token using the refresh token.
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<TokenData, AuthError> {
        let resp = self
            .http
            .post(GOOGLE_TOKEN_URL)
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            return Err(AuthError::RefreshFailed(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        #[derive(Deserialize)]
        struct RefreshResponse {
            access_token: String,
            expires_in: u64,
        }

        let parsed: RefreshResponse =
            serde_json::from_str(&body).map_err(|e| AuthError::RefreshFailed(e.to_string()))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(TokenData {
            access_token: parsed.access_token,
            refresh_token: refresh_token.to_string(),
            expires_at: now + parsed.expires_in,
        })
    }
}
