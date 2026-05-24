use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use tracing::info;

use crate::auth::token::TokenData;
use crate::auth::AuthError;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DRIVE_FILE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const LOCALHOST_REDIRECT: &str = "http://localhost";

/// PKCE code verifier — 128 random URL-safe characters.
fn generate_code_verifier() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::thread_rng();
    (0..128)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// SHA256 hash of verifier, base64url-encoded without padding.
fn generate_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Google OAuth 2.0 Authorization Code flow with PKCE.
///
/// Works on headless VPSes via paste-back: user opens the URL in any browser,
/// authorizes, then pastes the redirected localhost URL back into the terminal.
/// If a local port is reachable (e.g. on desktop), the redirect is caught
/// automatically by a tiny localhost HTTP server.
pub struct PkceFlow {
    client_id: String,
    client_secret: Option<String>,
    http: reqwest::Client,
}

impl PkceFlow {
    pub fn new(client_id: String, client_secret: Option<String>, http: reqwest::Client) -> Self {
        Self {
            client_id,
            client_secret,
            http,
        }
    }

    /// Run the PKCE authorization flow.
    ///
    /// 1. Bind a random localhost port (or fall back to paste-back)
    /// 2. Print auth URL for the user
    /// 3. Wait for the redirect callback or pasted URL
    /// 4. Exchange auth code for tokens
    pub async fn authorize(&self) -> Result<TokenData, AuthError> {
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);

        // Try to bind a localhost port for the redirect callback
        let listener = TcpListener::bind("127.0.0.1:0").ok();
        let port = listener.as_ref().map(|l| l.local_addr().unwrap().port());

        let redirect_uri = match port {
            Some(p) => format!("{}:{}", LOCALHOST_REDIRECT, p),
            None => format!("{}:8085", LOCALHOST_REDIRECT), // fallback for paste-back
        };

        let auth_url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent",
            GOOGLE_AUTH_URL,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&redirect_uri),
            urlencoding::encode(DRIVE_FILE_SCOPE),
            urlencoding::encode(&challenge),
        );

        println!("\n╔══════════════════════════════════════════════════╗");
        println!("║            GOOGLE AUTHORIZATION                  ║");
        println!("╠══════════════════════════════════════════════════╣");
        println!("║                                                  ║");
        println!("║  Open this URL in your browser:                  ║");
        println!("║                                                  ║");
        println!("╚══════════════════════════════════════════════════╝\n");
        println!("{}\n", auth_url);

        let auth_code = if let Some(tcp) = listener {
            // Set non-blocking so we can also accept paste-back
            tcp.set_nonblocking(true).ok();
            self.wait_for_callback_or_paste(tcp, &redirect_uri).await?
        } else {
            // No listener — pure paste-back
            println!("After authorizing, paste the full redirect URL here:");
            self.read_paste_back().await?
        };

        // Exchange code for tokens
        self.exchange_code(&auth_code, &verifier, &redirect_uri)
            .await
    }

    /// Wait for either a localhost HTTP callback or a pasted URL.
    async fn wait_for_callback_or_paste(
        &self,
        listener: TcpListener,
        redirect_uri: &str,
    ) -> Result<String, AuthError> {
        println!("Waiting for authorization...");
        println!("(If running on a headless server, paste the redirect URL below)\n");

        // Spawn a blocking task to read from stdin
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        let tx_paste = tx.clone();
        let _paste_handle = std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut line = String::new();
            if stdin.lock().read_line(&mut line).is_ok() {
                let _ = tx_paste.blocking_send(line.trim().to_string());
            }
        });

        // Also try to accept on the TCP listener
        let tx_http = tx;
        let redir = redirect_uri.to_string();
        let _http_handle = std::thread::spawn(move || {
            // Switch to blocking for accept
            listener.set_nonblocking(false).ok();
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_ok() {
                    // Parse: "GET /?code=AUTH_CODE&... HTTP/1.1"
                    if let Some(code) = extract_code_from_request(&request_line) {
                        // Send a nice HTML response
                        let body = "<html><body><h1>Authorization successful!</h1><p>You can close this tab and return to the terminal.</p></body></html>";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = tx_http.blocking_send(code);
                    }
                }
            }
            drop(redir);
        });

        // Wait for whichever completes first
        match rx.recv().await {
            Some(value) => {
                // Could be a pasted URL or the code directly from callback
                if value.starts_with("http") {
                    // Pasted URL — extract code
                    extract_code_from_url(&value).ok_or_else(|| {
                        AuthError::Unexpected(
                            "Could not find 'code' parameter in the pasted URL".to_string(),
                        )
                    })
                } else {
                    Ok(value)
                }
            }
            None => Err(AuthError::Unexpected(
                "No authorization response received".to_string(),
            )),
        }
    }

    /// Read a pasted URL from stdin.
    async fn read_paste_back(&self) -> Result<String, AuthError> {
        let line = tokio::task::spawn_blocking(|| {
            let stdin = std::io::stdin();
            let mut buf = String::new();
            stdin.lock().read_line(&mut buf).ok();
            buf.trim().to_string()
        })
        .await
        .map_err(|e| AuthError::Unexpected(e.to_string()))?;

        extract_code_from_url(&line).ok_or_else(|| {
            AuthError::Unexpected("Could not find 'code' parameter in the pasted URL".to_string())
        })
    }

    /// Exchange the authorization code for tokens.
    async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenData, AuthError> {
        let mut params = vec![
            ("client_id", self.client_id.as_str()),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ];

        let secret_str;
        if let Some(ref secret) = self.client_secret {
            secret_str = secret.clone();
            params.push(("client_secret", &secret_str));
        }

        let resp = self.http.post(GOOGLE_TOKEN_URL).form(&params).send().await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            return Err(AuthError::Unexpected(format!(
                "Token exchange failed (HTTP {}): {}",
                status, body
            )));
        }

        #[derive(Deserialize)]
        struct TokenResp {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: u64,
        }

        let parsed: TokenResp = serde_json::from_str(&body).map_err(|e| {
            AuthError::Unexpected(format!("Failed to parse token response: {}. Body: {}", e, body))
        })?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        info!("Authorization successful (PKCE)");
        Ok(TokenData {
            access_token: parsed.access_token,
            refresh_token: parsed
                .refresh_token
                .unwrap_or_else(|| String::from("none")),
            expires_at: now + parsed.expires_in,
        })
    }
}

/// Extract the `code` query parameter from an HTTP request line.
/// E.g. "GET /?code=4/abc&scope=... HTTP/1.1"
fn extract_code_from_request(request_line: &str) -> Option<String> {
    let path = request_line.split_whitespace().nth(1)?;
    extract_code_from_query(path.split('?').nth(1)?)
}

/// Extract the `code` query parameter from a full URL.
fn extract_code_from_url(url: &str) -> Option<String> {
    let query = url.split('?').nth(1)?;
    extract_code_from_query(query)
}

/// Extract `code` from a query string like `code=VALUE&scope=...`
fn extract_code_from_query(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        if kv.next()? == "code" {
            return kv.next().map(|v| urlencoding::decode(v).unwrap_or_default().to_string());
        }
    }
    None
}
