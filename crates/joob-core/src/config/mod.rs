mod profile;

pub use profile::{Profile, ProfileError};

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Base64-encoded 32-byte tunnel secret
    pub tunnel_secret: String,
    pub drive_folder_id: String,
    #[serde(default = "default_google_ip")]
    pub google_ip: String,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    pub oauth: OAuthTokens,
}

fn default_google_ip() -> String {
    "216.239.38.120".to_string()
}

fn default_socks_port() -> u16 {
    1080
}

fn default_http_port() -> u16 {
    8080
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitConfig {
    /// Base64-encoded 32-byte tunnel secret
    pub tunnel_secret: String,
    pub drive_folder_id: String,
    pub oauth: OAuthTokens,
    pub watch_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    pub refresh_token: String,
}

impl ClientConfig {
    /// Load a ClientConfig from a JSON file.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&data)?;
        Ok(config)
    }

    /// Save this ClientConfig to a JSON file.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

impl ExitConfig {
    /// Load an ExitConfig from a JSON file.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&data)?;
        Ok(config)
    }

    /// Save this ExitConfig to a JSON file.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn sample_oauth() -> OAuthTokens {
        OAuthTokens {
            client_id: "test-client-id".to_string(),
            client_secret: Some("test-client-secret".to_string()),
            refresh_token: "test-refresh-token".to_string(),
        }
    }

    fn sample_client_config() -> ClientConfig {
        ClientConfig {
            tunnel_secret: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &[0u8; 32],
            ),
            drive_folder_id: "folder-123".to_string(),
            google_ip: "216.239.38.120".to_string(),
            socks_port: 1080,
            http_port: 8080,
            oauth: sample_oauth(),
        }
    }

    #[test]
    fn test_client_config_save_load_roundtrip() {
        let config = sample_client_config();
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        config.save_to_file(&path).unwrap();

        let loaded = ClientConfig::load_from_file(&path).unwrap();
        assert_eq!(loaded.tunnel_secret, config.tunnel_secret);
        assert_eq!(loaded.drive_folder_id, config.drive_folder_id);
        assert_eq!(loaded.google_ip, config.google_ip);
        assert_eq!(loaded.socks_port, config.socks_port);
        assert_eq!(loaded.http_port, config.http_port);
        assert_eq!(loaded.oauth.client_id, config.oauth.client_id);
    }

    #[test]
    fn test_exit_config_save_load_roundtrip() {
        let config = ExitConfig {
            tunnel_secret: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &[1u8; 32],
            ),
            drive_folder_id: "exit-folder".to_string(),
            oauth: sample_oauth(),
            watch_port: Some(9090),
        };
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        config.save_to_file(&path).unwrap();

        let loaded = ExitConfig::load_from_file(&path).unwrap();
        assert_eq!(loaded.tunnel_secret, config.tunnel_secret);
        assert_eq!(loaded.watch_port, Some(9090));
    }

    #[test]
    fn test_client_config_defaults() {
        // JSON without google_ip, socks_port, http_port should use defaults
        let json = r#"{
            "tunnel_secret": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "drive_folder_id": "folder-123",
            "oauth": {
                "client_id": "cid",
                "client_secret": null,
                "refresh_token": "rt"
            }
        }"#;
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();

        let config = ClientConfig::load_from_file(tmp.path()).unwrap();
        assert_eq!(config.google_ip, "216.239.38.120");
        assert_eq!(config.socks_port, 1080);
        assert_eq!(config.http_port, 8080);
    }
}
