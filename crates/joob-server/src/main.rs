use clap::Parser;
use joob_core::auth::{DeviceCodeFlow, OAuthConfig, PkceFlow, TokenStore};
use joob_core::config::{ClientConfig, ExitConfig, OAuthTokens, Profile};
use joob_core::crypto::TunnelKey;
use joob_core::drive::{DriveClient, FileRotator};
use joob_core::fronting::FrontedClient;
use joob_core::tunnel::ExitTunnel;
use std::sync::Arc;

/// Convert `Box<dyn Error>` into `anyhow::Error`.
fn boxerr(e: Box<dyn std::error::Error>) -> anyhow::Error {
    anyhow::anyhow!("{}", e)
}

#[derive(Parser)]
#[command(name = "joob-server", version, about = "Joob tunnel server — runs on your VPS")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Interactive setup: OAuth login, create Drive folder, generate profile
    Setup {
        /// Google OAuth client ID
        #[arg(long)]
        client_id: Option<String>,
        /// Google OAuth client secret (only needed for device-code flow)
        #[arg(long)]
        client_secret: Option<String>,
        /// OAuth flow: "pkce" (default, Desktop app) or "device" (TV/Limited Input app)
        #[arg(long, default_value = "pkce")]
        oauth_flow: String,
        /// Google edge IP for domain fronting (client-side)
        #[arg(long, default_value = "216.239.38.120")]
        google_ip: String,
    },
    /// Start exit tunnel daemon
    Run {
        /// Path to exit config file
        #[arg(long, default_value = "exit.json")]
        config: String,
    },
    /// Clean up old Drive files
    Cleanup {
        #[arg(long, default_value = "exit.json")]
        config: String,
    },
    /// Revoke OAuth token
    Revoke {
        #[arg(long, default_value = "exit.json")]
        config: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Setup {
            client_id,
            client_secret,
            oauth_flow,
            google_ip,
        } => {
            run_setup(client_id, client_secret, &oauth_flow, google_ip).await?;
        }
        Commands::Run { config } => {
            let exit_config = ExitConfig::load_from_file(&config).map_err(boxerr)?;
            ExitTunnel::start(exit_config)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
        Commands::Cleanup { config } => {
            run_cleanup(&config).await?;
        }
        Commands::Revoke { config: _ } => {
            run_revoke().await?;
        }
    }

    Ok(())
}

async fn run_setup(
    client_id: Option<String>,
    client_secret: Option<String>,
    oauth_flow: &str,
    google_ip: String,
) -> anyhow::Result<()> {
    println!("╔══════════════════════════════════════════╗");
    println!("║          JOOB EXIT SETUP                 ║");
    println!("╚══════════════════════════════════════════╝\n");

    // Read client_id from stdin if not provided
    let cid = match client_id {
        Some(id) => id,
        None => {
            println!("Enter your Google OAuth Client ID:");
            let mut buf = String::new();
            std::io::stdin().read_line(&mut buf)?;
            buf.trim().to_string()
        }
    };

    if cid.is_empty() {
        anyhow::bail!("Client ID is required. Create one at https://console.cloud.google.com/apis/credentials");
    }

    let oauth_config = OAuthConfig {
        client_id: cid,
        client_secret: client_secret.clone(),
    };

    // Step 1: OAuth login
    println!("Step 1/4: Google authentication ({} flow)...", oauth_flow);
    let http = FrontedClient::build_direct()?;

    let token = match oauth_flow {
        "device" => {
            if oauth_config.client_secret.is_none() {
                anyhow::bail!("--client-secret is required for device-code flow. Use --oauth-flow pkce to skip it.");
            }
            let flow = DeviceCodeFlow::new(oauth_config.clone(), http.clone());
            flow.authorize().await?
        }
        _ => {
            // PKCE flow (default)
            let flow = PkceFlow::new(
                oauth_config.client_id.clone(),
                oauth_config.client_secret.clone(),
                http.clone(),
            );
            flow.authorize().await?
        }
    };

    // Save token
    let token_path = std::path::PathBuf::from("exit_token.json");
    let token_store = Arc::new(TokenStore::new(token_path));
    token_store.store(token.clone()).await?;

    // Step 2: Create Drive folder
    println!("\nStep 2/4: Creating Drive folder...");
    let drive = Arc::new(DriveClient::new(
        http,
        token_store.clone(),
        oauth_config.clone(),
    ));
    let folder_id = drive.create_folder("joob-tunnel", None).await?;
    println!("  Folder ID: {}", folder_id);

    // Step 3: Generate tunnel secret
    println!("\nStep 3/4: Generating tunnel secret...");
    let tunnel_key = TunnelKey::generate();
    let tunnel_secret = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        tunnel_key.as_bytes(),
    );

    // Step 4: Save exit config
    let exit_config = ExitConfig {
        tunnel_secret: tunnel_secret.clone(),
        drive_folder_id: folder_id.clone(),
        oauth: OAuthTokens {
            client_id: oauth_config.client_id.clone(),
            client_secret: oauth_config.client_secret.clone(),
            refresh_token: token.refresh_token.clone(),
        },
        watch_port: None,
    };
    exit_config.save_to_file("exit.json").map_err(boxerr)?;
    println!("\nStep 4/4: Config saved to exit.json");

    // Generate client profile
    let client_config = ClientConfig {
        tunnel_secret: tunnel_secret.clone(),
        drive_folder_id: folder_id,
        google_ip,
        socks_port: 1080,
        http_port: 8080,
        oauth: OAuthTokens {
            client_id: oauth_config.client_id,
            client_secret: oauth_config.client_secret,
            refresh_token: token.refresh_token,
        },
    };
    let profile = Profile::encode(&client_config);

    println!("\n╔══════════════════════════════════════════╗");
    println!("║          SETUP COMPLETE ✓                ║");
    println!("╠══════════════════════════════════════════╣");
    println!("║                                          ║");
    println!("║  Share this profile with the client:     ║");
    println!("║                                          ║");
    println!("╚══════════════════════════════════════════╝\n");
    println!("{}\n", profile);
    println!("Run the server:  joob-server run");
    println!(
        "Connect from client: joob-client connect --profile \"{}...\"",
        &profile[..50.min(profile.len())]
    );

    Ok(())
}

async fn run_cleanup(config_path: &str) -> anyhow::Result<()> {
    let config = ExitConfig::load_from_file(config_path).map_err(boxerr)?;
    let http = FrontedClient::build_direct()?;
    let oauth_config = OAuthConfig {
        client_id: config.oauth.client_id.clone(),
        client_secret: config.oauth.client_secret.clone(),
    };
    let token_store = Arc::new(TokenStore::new("exit_token.json".into()));
    let drive = Arc::new(DriveClient::new(http, token_store, oauth_config));
    let rotator = FileRotator::new(drive, config.drive_folder_id);

    let deleted_up = rotator.cleanup_all("up").await?;
    let deleted_dn = rotator.cleanup_all("dn").await?;
    println!(
        "Cleaned up {} upstream + {} downstream files",
        deleted_up, deleted_dn
    );
    Ok(())
}

async fn run_revoke() -> anyhow::Result<()> {
    let token_store = TokenStore::new("exit_token.json".into());
    token_store.clear().await?;
    println!("Token revoked. Run 'joob-server setup' to re-authenticate.");
    Ok(())
}
