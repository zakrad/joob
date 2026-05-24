use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Configuration for domain-fronted access to Google APIs.
///
/// Makes all HTTPS requests to `www.googleapis.com` resolve to a Google edge IP.
/// DPI sees a standard HTTPS connection to a Google IP address.
#[derive(Debug, Clone)]
pub struct FrontingConfig {
    /// Google edge IP to connect to
    pub google_ip: IpAddr,
    /// The SNI hostname (what DPI sees in the TLS ClientHello)
    pub sni_host: String,
    /// The actual API host (used in HTTP Host header)
    pub api_host: String,
}

impl Default for FrontingConfig {
    fn default() -> Self {
        Self {
            google_ip: IpAddr::V4(Ipv4Addr::new(216, 239, 38, 120)),
            sni_host: "www.google.com".into(),
            api_host: "www.googleapis.com".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FrontingError {
    #[error("failed to build HTTP client: {0}")]
    ClientBuild(reqwest::Error),
    #[error("invalid SNI hostname: {0}")]
    InvalidSni(String),
}

/// Builder for domain-fronted HTTP clients.
///
/// Creates `reqwest::Client` instances that route Google API traffic through
/// a Google edge IP. On restricted networks, DPI only sees a standard HTTPS
/// connection to a well-known Google IP address.
pub struct FrontedClient;

impl FrontedClient {
    /// Build a `reqwest::Client` for domain-fronted access to Google APIs.
    ///
    /// The client resolves `www.googleapis.com`, `oauth2.googleapis.com`, and
    /// `accounts.google.com` to the configured Google edge IP. Since Google
    /// serves all these domains from the same edge infrastructure and their
    /// certificates cover all of them, TLS validation succeeds normally.
    ///
    /// DPI sees a standard HTTPS connection to a Google IP — indistinguishable
    /// from normal Google traffic.
    pub fn build(config: &FrontingConfig) -> Result<reqwest::Client, FrontingError> {
        let addr = SocketAddr::new(config.google_ip, 443);

        reqwest::Client::builder()
            .resolve("www.googleapis.com", addr)
            .resolve("oauth2.googleapis.com", addr)
            .resolve("accounts.google.com", addr)
            .build()
            .map_err(FrontingError::ClientBuild)
    }

    /// Build a direct (non-fronted) `reqwest::Client`.
    ///
    /// Used by the exit side which has unrestricted internet access and does
    /// not need domain fronting.
    pub fn build_direct() -> Result<reqwest::Client, FrontingError> {
        reqwest::Client::builder()
            .build()
            .map_err(FrontingError::ClientBuild)
    }

    /// Build a `reqwest::Client` for use with a Cloudflare Worker frontend.
    ///
    /// The caller is responsible for rewriting Google API URLs to the Worker's
    /// base URL — this builder only sets up the optional shared-secret header.
    /// No DNS pinning: Cloudflare IPs are reachable from everywhere joob runs.
    pub fn build_frontend(auth_token: Option<&str>) -> Result<reqwest::Client, FrontingError> {
        let mut builder = reqwest::Client::builder();
        if let Some(token) = auth_token {
            let mut headers = reqwest::header::HeaderMap::new();
            let value = reqwest::header::HeaderValue::from_str(token)
                .map_err(|e| FrontingError::InvalidSni(e.to_string()))?;
            headers.insert("X-Joob-Auth", value);
            builder = builder.default_headers(headers);
        }
        builder.build().map_err(FrontingError::ClientBuild)
    }
}
