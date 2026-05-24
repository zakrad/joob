use crate::auth::{DeviceCodeFlow, OAuthConfig, TokenStore};
use crate::config::ExitConfig;
use crate::crypto::{Cipher, TunnelKey};
use crate::drive::{DriveClient, Downloader, QuotaTracker, Uploader};
use crate::fronting::FrontedClient;
use crate::mux::{Mux, StreamHandle};
use crate::transport::Pipe;
use base64::Engine;
use bytes::Bytes;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

/// Exit-side tunnel. Receives multiplexed streams from the client via Drive,
/// dials the actual target hosts, and bridges traffic.
pub struct ExitTunnel;

impl ExitTunnel {
    pub async fn start(
        config: ExitConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting Joob exit tunnel...");

        // 1. Build direct HTTP client (exit has unrestricted internet)
        let http = FrontedClient::build_direct()?;

        // 2. Set up auth
        let oauth_config = OAuthConfig {
            client_id: config.oauth.client_id.clone(),
            client_secret: config.oauth.client_secret.clone(),
        };
        let token_store = Arc::new(TokenStore::new(dirs_config_path("exit_token.json")));

        if token_store.needs_refresh(0).await {
            let flow = DeviceCodeFlow::new(oauth_config.clone(), http.clone());
            let token = flow.authorize().await?;
            token_store.store(token).await?;
        }

        // 3. Create Drive client
        let drive = Arc::new(DriveClient::new(http, token_store, oauth_config));

        // 4. Create ciphers (reverse direction from client)
        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&config.tunnel_secret)
            .map_err(|e| format!("invalid tunnel secret: {}", e))?;
        let key_arr: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| "tunnel secret must be 32 bytes")?;
        let tunnel_key = TunnelKey::from_bytes(key_arr);

        // Exit receives on "up" (upstream from client), sends on "dn" (downstream to client)
        let recv_cipher = Arc::new(Cipher::new(&tunnel_key, [0x00, 0x00, 0x00, 0x01]));
        let send_cipher = Arc::new(Cipher::new(&tunnel_key, [0x00, 0x00, 0x00, 0x02]));

        // 5. Create uploader (dn) + downloader (up)
        let uploader = Arc::new(Uploader::new(
            Arc::clone(&drive),
            config.drive_folder_id.clone(),
            "dn".to_string(),
        ));
        let downloader = Arc::new(Downloader::new(
            Arc::clone(&drive),
            config.drive_folder_id.clone(),
            "up".to_string(),
        ));

        // 6. Create mux
        let mux = Arc::new(Mux::new());

        // 7. Set up stream handler — dial targets when client opens streams
        mux.set_on_stream_open(move |handle: StreamHandle| {
            tokio::spawn(async move {
                Self::handle_stream(handle).await;
            });
        })
        .await;

        // 8. Create quota tracker
        let quota = Arc::new(QuotaTracker::new());

        // 9. Start transport pipe
        let _pipe = Pipe::start(
            Arc::clone(&mux),
            uploader,
            downloader,
            send_cipher,
            recv_cipher,
            quota,
        );

        info!("Joob exit tunnel running. Waiting for client connections...");

        // Wait for shutdown
        tokio::signal::ctrl_c().await?;
        info!("Shutting down...");

        Ok(())
    }

    /// Handle an incoming stream: dial the target and bridge data.
    async fn handle_stream(mut handle: StreamHandle) {
        let target = handle.target.clone();
        let stream_id = handle.stream_id;

        debug!(stream_id, target = %target, "dialing target");

        // Connect to the target
        let stream = match TcpStream::connect(&target).await {
            Ok(s) => s,
            Err(e) => {
                warn!(stream_id, target = %target, error = %e, "failed to dial target");
                return;
            }
        };

        let (mut tcp_read, mut tcp_write) = stream.into_split();

        // TCP → mux (target's response back to client)
        let handle_tx = handle.tx_clone();
        let tcp_to_mux = tokio::spawn(async move {
            let mut buf = vec![0u8; 32768];
            loop {
                match tcp_read.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if handle_tx
                            .send(Bytes::copy_from_slice(&buf[..n]))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // mux → TCP (client's data to target)
        let mux_to_tcp = tokio::spawn(async move {
            while let Some(data) = handle.recv().await {
                if tcp_write.write_all(&data).await.is_err() {
                    break;
                }
            }
        });

        tokio::select! {
            _ = tcp_to_mux => {}
            _ = mux_to_tcp => {}
        }

        debug!(stream_id, "stream handler done");
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
