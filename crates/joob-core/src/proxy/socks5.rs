use crate::mux::Mux;
use bytes::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};

pub struct Socks5Server {
    mux: Arc<Mux>,
}

impl Socks5Server {
    pub fn new(mux: Arc<Mux>) -> Self {
        Self { mux }
    }

    pub async fn run(
        &self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        info!(addr = %addr, "SOCKS5 server listening");

        loop {
            let (stream, peer) = listener.accept().await?;
            let mux = Arc::clone(&self.mux);
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, peer, mux).await {
                    debug!(peer = %peer, error = %e, "SOCKS5 connection error");
                }
            });
        }
    }

    async fn handle_connection(
        mut stream: TcpStream,
        peer: SocketAddr,
        mux: Arc<Mux>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Step 1: Greeting
        let mut buf = [0u8; 258];
        stream.read_exact(&mut buf[..2]).await?;

        let ver = buf[0];
        let nmethods = buf[1] as usize;

        if ver != 0x05 {
            return Err(format!("unsupported SOCKS version: {}", ver).into());
        }

        stream.read_exact(&mut buf[..nmethods]).await?;

        // Reply: no auth required
        stream.write_all(&[0x05, 0x00]).await?;

        // Step 2: Connection request
        stream.read_exact(&mut buf[..4]).await?;

        let ver = buf[0];
        let cmd = buf[1];
        // buf[2] is reserved
        let atype = buf[3];

        if ver != 0x05 || cmd != 0x01 {
            // Only support CONNECT
            stream
                .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            return Err("unsupported command".into());
        }

        // Parse address
        let target = match atype {
            0x01 => {
                // IPv4
                stream.read_exact(&mut buf[..4]).await?;
                let ip = format!("{}.{}.{}.{}", buf[0], buf[1], buf[2], buf[3]);
                stream.read_exact(&mut buf[..2]).await?;
                let port = u16::from_be_bytes([buf[0], buf[1]]);
                format!("{}:{}", ip, port)
            }
            0x03 => {
                // Domain name
                stream.read_exact(&mut buf[..1]).await?;
                let len = buf[0] as usize;
                stream.read_exact(&mut buf[..len]).await?;
                let domain = String::from_utf8_lossy(&buf[..len]).to_string();
                stream.read_exact(&mut buf[..2]).await?;
                let port = u16::from_be_bytes([buf[0], buf[1]]);
                format!("{}:{}", domain, port)
            }
            0x04 => {
                // IPv6
                stream.read_exact(&mut buf[..16]).await?;
                let mut parts = Vec::new();
                for i in 0..8 {
                    parts.push(format!(
                        "{:x}",
                        u16::from_be_bytes([buf[i * 2], buf[i * 2 + 1]])
                    ));
                }
                let ip = parts.join(":");
                stream.read_exact(&mut buf[..2]).await?;
                let port = u16::from_be_bytes([buf[0], buf[1]]);
                format!("[{}]:{}", ip, port)
            }
            _ => {
                stream
                    .write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await?;
                return Err("unsupported address type".into());
            }
        };

        debug!(peer = %peer, target = %target, "SOCKS5 CONNECT");

        // Open mux stream
        let mut mux_stream = match mux.open_stream(&target).await {
            Ok(s) => s,
            Err(e) => {
                stream
                    .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await?;
                return Err(format!("failed to open stream: {}", e).into());
            }
        };

        // Send success response
        stream
            .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await?;

        // Bridge TCP ↔ mux stream
        let (mut tcp_read, mut tcp_write) = stream.into_split();
        let stream_id = mux_stream.stream_id;
        let mux_for_close = Arc::clone(&mux);

        // TCP → mux
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

        // mux → TCP
        let mux_to_tcp = tokio::spawn(async move {
            while let Some(data) = mux_stream.recv().await {
                if tcp_write.write_all(&data).await.is_err() {
                    break;
                }
            }
        });

        // Wait for either direction to finish
        tokio::select! {
            _ = tcp_to_mux => {}
            _ = mux_to_tcp => {}
        }

        mux_for_close.close_stream(stream_id).await;
        debug!(stream_id, "SOCKS5 stream closed");
        Ok(())
    }
}
