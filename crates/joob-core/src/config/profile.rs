use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use thiserror::Error;

use super::ClientConfig;

const PROFILE_PREFIX: &str = "joob://";

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("invalid profile prefix: expected 'joob://'")]
    InvalidPrefix,
    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("json parse error: {0}")]
    JsonParse(#[from] serde_json::Error),
}

/// Profile encodes/decodes a `ClientConfig` into a portable `joob://` URL-safe string.
pub struct Profile;

impl Profile {
    /// Encode a `ClientConfig` into a `joob://`-prefixed URL-safe base64 string.
    pub fn encode(config: &ClientConfig) -> String {
        let json = serde_json::to_string(config).expect("ClientConfig must be serializable");
        let b64 = URL_SAFE_NO_PAD.encode(json.as_bytes());
        format!("{}{}", PROFILE_PREFIX, b64)
    }

    /// Decode a `joob://`-prefixed URL-safe base64 string back into a `ClientConfig`.
    pub fn decode(s: &str) -> Result<ClientConfig, ProfileError> {
        let payload = s.strip_prefix(PROFILE_PREFIX).ok_or(ProfileError::InvalidPrefix)?;
        let bytes = URL_SAFE_NO_PAD.decode(payload)?;
        let config: ClientConfig = serde_json::from_slice(&bytes)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OAuthTokens;

    fn sample_config() -> ClientConfig {
        ClientConfig {
            tunnel_secret: base64::engine::general_purpose::STANDARD.encode(&[42u8; 32]),
            drive_folder_id: "test-folder-id".to_string(),
            google_ip: "216.239.38.120".to_string(),
            socks_port: 1080,
            http_port: 8080,
            oauth: OAuthTokens {
                client_id: "my-client-id".to_string(),
                client_secret: Some("my-client-secret".to_string()),
                refresh_token: "my-refresh-token".to_string(),
            },
            drive_frontend: None,
        }
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let config = sample_config();
        let encoded = Profile::encode(&config);

        assert!(encoded.starts_with("joob://"));

        let decoded = Profile::decode(&encoded).unwrap();
        assert_eq!(decoded.tunnel_secret, config.tunnel_secret);
        assert_eq!(decoded.drive_folder_id, config.drive_folder_id);
        assert_eq!(decoded.google_ip, config.google_ip);
        assert_eq!(decoded.socks_port, config.socks_port);
        assert_eq!(decoded.http_port, config.http_port);
        assert_eq!(decoded.oauth.client_id, config.oauth.client_id);
        assert_eq!(decoded.oauth.client_secret, config.oauth.client_secret);
        assert_eq!(decoded.oauth.refresh_token, config.oauth.refresh_token);
    }

    #[test]
    fn test_invalid_prefix() {
        let result = Profile::decode("http://somethingelse");
        assert!(result.is_err());
        match result.unwrap_err() {
            ProfileError::InvalidPrefix => {}
            other => panic!("expected InvalidPrefix, got: {:?}", other),
        }
    }

    #[test]
    fn test_corrupted_base64() {
        // Valid prefix but invalid base64 content
        let result = Profile::decode("joob://!!!not-valid-base64!!!");
        assert!(result.is_err());
        match result.unwrap_err() {
            ProfileError::Base64Decode(_) => {}
            other => panic!("expected Base64Decode, got: {:?}", other),
        }
    }

    #[test]
    fn test_valid_base64_invalid_json() {
        // Valid prefix + valid base64, but not valid JSON
        let b64 = URL_SAFE_NO_PAD.encode(b"not json at all");
        let input = format!("joob://{}", b64);
        let result = Profile::decode(&input);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProfileError::JsonParse(_) => {}
            other => panic!("expected JsonParse, got: {:?}", other),
        }
    }
}
