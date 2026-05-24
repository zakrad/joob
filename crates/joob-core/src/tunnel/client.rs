use crate::auth::{DeviceCodeFlow, OAuthConfig, TokenStore};
use crate::config::ClientConfig;
use crate::crypto::{Cipher, TunnelKey};
use crate::drive::{DriveClient, Downloader, QuotaTracker, Uploader};
use crate::fronting::{FrontedClient, FrontingConfig};
use crate::mux::Mux;
use crate::proxy::{HttpProxy, Socks5Server};
use crate::transport::Pipe;
use base64::Engine;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tracing::{error, info};

/// Client-side tunnel. Runs SOCKS5/HTTP proxy, multiplexes traffic through
/// Google Drive via domain-fronted HTTPS.
pub struct ClientTunnel;

impl ClientTunnel {
    /// Start the client tunnel. If `on_ready` is provided, it is called once the
    /// proxy servers are bound and the transport pipe is running.
    pub async fn start(
        config: ClientConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Self::start_with_callback(config, None::<fn()>).await
    }

    /// Start the client tunnel with an optional ready callback.
    pub async fn start_with_callback<F: FnOnce() + Send + 'static>(
        config: ClientConfig,
        on_ready: Option<F>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting Joob client tunnel...");

        // 1. Build HTTP client + Drive client.
        //
        // Two modes:
        // - drive_frontend set: route all Google traffic through a Cloudflare
        //   Worker. No DNS pinning needed (Cloudflare IPs are reachable).
        // - Otherwise: legacy domain-fronting that pins googleapis.com to a
        //   Google edge IP.
        let oauth_config = OAuthConfig {
            client_id: config.oauth.client_id.clone(),
            client_secret: config.oauth.client_secret.clone(),
        };

        let token_store = Arc::new(TokenStore::new(dirs_config_path("client_token.json")));

        let drive = if let Some(frontend) = &config.drive_frontend {
            info!("Using Cloudflare Worker frontend at {}", frontend.base_url);
            let http = FrontedClient::build_frontend(frontend.auth_token.as_deref())?;

            if token_store.needs_refresh(60).await {
                info!("Refreshing access token via frontend...");
                let flow = DeviceCodeFlow::with_frontend(
                    oauth_config.clone(),
                    http.clone(),
                    &frontend.base_url,
                );
                let token = flow
                    .refresh_token(&config.oauth.refresh_token)
                    .await
                    .map_err(|e| format!("Token refresh failed: {}. Re-run setup on the server to get a new profile.", e))?;
                token_store.store(token).await?;
            }

            Arc::new(DriveClient::with_frontend(
                http,
                Arc::clone(&token_store),
                oauth_config,
                &frontend.base_url,
            ))
        } else {
            let google_ip = config
                .google_ip
                .parse::<IpAddr>()
                .unwrap_or(IpAddr::V4(Ipv4Addr::new(216, 239, 38, 120)));
            let fronting_config = FrontingConfig {
                google_ip,
                ..Default::default()
            };
            let http = FrontedClient::build(&fronting_config)?;

            if token_store.needs_refresh(60).await {
                info!("Refreshing access token...");
                let flow = DeviceCodeFlow::new(oauth_config.clone(), http.clone());
                let token = flow
                    .refresh_token(&config.oauth.refresh_token)
                    .await
                    .map_err(|e| format!("Token refresh failed: {}. Re-run setup on the server to get a new profile.", e))?;
                token_store.store(token).await?;
            }

            Arc::new(DriveClient::new(http, Arc::clone(&token_store), oauth_config))
        };

        // 4. Create tunnel secret + ciphers
        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&config.tunnel_secret)
            .map_err(|e| format!("invalid tunnel secret: {}", e))?;
        let key_arr: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| "tunnel secret must be 32 bytes")?;
        let tunnel_key = TunnelKey::from_bytes(key_arr);

        // Client sends on "up" (upstream), receives on "dn" (downstream)
        let send_cipher = Arc::new(Cipher::new(&tunnel_key, [0x00, 0x00, 0x00, 0x01]));
        let recv_cipher = Arc::new(Cipher::new(&tunnel_key, [0x00, 0x00, 0x00, 0x02]));

        // 5. Create uploader + downloader
        let uploader = Arc::new(Uploader::new(
            Arc::clone(&drive),
            config.drive_folder_id.clone(),
            "up".to_string(),
        ));
        let downloader = Arc::new(Downloader::new(
            Arc::clone(&drive),
            config.drive_folder_id.clone(),
            "dn".to_string(),
        ));

        // 6. Create mux
        let mux = Arc::new(Mux::new());

        // 7. Create quota tracker
        let quota = Arc::new(QuotaTracker::new());

        // 8. Start transport pipe
        let _pipe = Pipe::start(
            Arc::clone(&mux),
            uploader,
            downloader,
            send_cipher,
            recv_cipher,
            quota,
        );

        // 9. Start proxy servers
        let socks_addr: SocketAddr = format!("127.0.0.1:{}", config.socks_port).parse()?;
        let http_addr: SocketAddr = format!("127.0.0.1:{}", config.http_port).parse()?;

        let socks5 = Socks5Server::new(Arc::clone(&mux));
        let http_proxy = HttpProxy::new(Arc::clone(&mux));

        info!(socks = %socks_addr, http = %http_addr, "proxy servers starting");

        let socks_handle = tokio::spawn(async move {
            if let Err(e) = socks5.run(socks_addr).await {
                error!(error = %e, "SOCKS5 server error");
            }
        });

        let http_handle = tokio::spawn(async move {
            if let Err(e) = http_proxy.run(http_addr).await {
                error!(error = %e, "HTTP proxy error");
            }
        });

        info!("Joob client tunnel running. Press Ctrl+C to stop.");

        // Signal ready
        if let Some(cb) = on_ready {
            cb();
        }

        // Wait for shutdown
        tokio::signal::ctrl_c().await?;
        info!("Shutting down...");

        socks_handle.abort();
        http_handle.abort();

        Ok(())
    }
}

fn dirs_config_path(filename: &str) -> std::path::PathBuf {
    let mut path = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    path.push(".config");
    path.push("joob");
    path.push(filename);
    path
}
