use crate::mux::Mux;
use bytes::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};

pub struct HttpProxy {
    mux: Arc<Mux>,
}

impl HttpProxy {
    pub fn new(mux: Arc<Mux>) -> Self {
        Self { mux }
    }

    pub async fn run(
        &self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        info!(addr = %addr, "HTTP proxy listening");

        loop {
            let (stream, peer) = listener.accept().await?;
            let mux = Arc::clone(&self.mux);
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, peer, mux).await {
                    debug!(peer = %peer, error = %e, "HTTP proxy error");
                }
            });
        }
    }

    async fn handle_connection(
        stream: TcpStream,
        peer: SocketAddr,
        mux: Arc<Mux>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut reader = BufReader::new(stream);

        // Read request line
        let mut request_line = String::new();
        reader.read_line(&mut request_line).await?;
        let parts: Vec<&str> = request_line.trim().split_whitespace().collect();

        if parts.len() < 3 {
            return Err("invalid request line".into());
        }

        let method = parts[0];
        let target = parts[1];

        if method != "CONNECT" {
            // Only support CONNECT for now
            let response = "HTTP/1.1 405 Method Not Allowed\r\n\r\n";
            reader.get_mut().write_all(response.as_bytes()).await?;
            return Ok(());
        }

        // Read and discard remaining headers
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            if line.trim().is_empty() {
                break;
            }
        }

        debug!(peer = %peer, target = %target, "HTTP CONNECT");

        // Open mux stream
        let mut mux_stream = match mux.open_stream(target).await {
            Ok(s) => s,
            Err(e) => {
                let response = "HTTP/1.1 502 Bad Gateway\r\n\r\n";
                reader.get_mut().write_all(response.as_bytes()).await?;
                return Err(format!("failed to open stream: {}", e).into());
            }
        };

        // Send 200 Connection Established
        reader
            .get_mut()
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;

        let stream = reader.into_inner();
        let (mut tcp_read, mut tcp_write) = stream.into_split();
        let stream_id = mux_stream.stream_id;
        let mux_for_close = Arc::clone(&mux);

        let mux_stream_tx = mux_stream.tx_clone();
        let tcp_to_mux = tokio::spawn(async move {
            let mut buf = vec![0u8; 32768];
            loop {
                match tcp_read.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if mux_stream_tx
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

        let mux_to_tcp = tokio::spawn(async move {
            while let Some(data) = mux_stream.recv().await {
                if tcp_write.write_all(&data).await.is_err() {
                    break;
                }
            }
        });

        tokio::select! {
            _ = tcp_to_mux => {}
            _ = mux_to_tcp => {}
        }

        mux_for_close.close_stream(stream_id).await;
        Ok(())
    }
}
