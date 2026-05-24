use clap::Parser;
use joob_core::config::{ClientConfig, Profile};
use joob_core::tunnel::ClientTunnel;

/// Convert `Box<dyn Error>` into `anyhow::Error`.
fn boxerr(e: Box<dyn std::error::Error>) -> anyhow::Error {
    anyhow::anyhow!("{}", e)
}

#[derive(Parser)]
#[command(name = "joob-client", version, about = "Joob tunnel client — runs on your local machine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Connect to tunnel
    Connect {
        /// joob:// profile string
        #[arg(long)]
        profile: Option<String>,
        /// Path to config file
        #[arg(long)]
        config: Option<String>,
        /// Override SOCKS5 port
        #[arg(long)]
        socks_port: Option<u16>,
        /// Override HTTP proxy port
        #[arg(long)]
        http_port: Option<u16>,
    },
    /// Import a profile and save to config file
    Import {
        /// joob:// profile string
        #[arg(long)]
        profile: String,
        /// Output config file path
        #[arg(long, default_value = "client.json")]
        output: String,
    },
    /// Test tunnel connectivity
    Test {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        config: Option<String>,
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
        Commands::Connect {
            profile,
            config,
            socks_port,
            http_port,
        } => {
            let mut client_config = load_config(profile, config)?;
            if let Some(p) = socks_port {
                client_config.socks_port = p;
            }
            if let Some(p) = http_port {
                client_config.http_port = p;
            }

            println!("╔══════════════════════════════════════════╗");
            println!("║          JOOB CLIENT                     ║");
            println!("╠══════════════════════════════════════════╣");
            println!(
                "║  SOCKS5: 127.0.0.1:{:<21}║",
                client_config.socks_port
            );
            println!(
                "║  HTTP:   127.0.0.1:{:<21}║",
                client_config.http_port
            );
            println!("╚══════════════════════════════════════════╝\n");

            ClientTunnel::start(client_config)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
        Commands::Import { profile, output } => {
            let config = Profile::decode(&profile)?;
            config.save_to_file(&output).map_err(boxerr)?;
            println!("Profile imported and saved to {}", output);
            println!("Run: joob-client connect --config {}", output);
        }
        Commands::Test { profile, config } => {
            let client_config = load_config(profile, config)?;
            println!("Config loaded successfully.");
            println!("  Drive folder: {}", client_config.drive_folder_id);
            println!("  Google IP: {}", client_config.google_ip);
            println!("  SOCKS port: {}", client_config.socks_port);
            println!("  HTTP port: {}", client_config.http_port);
            // TODO: ping test through tunnel
            println!("\nTo connect: joob-client connect --config client.json");
        }
    }

    Ok(())
}

fn load_config(
    profile: Option<String>,
    config: Option<String>,
) -> anyhow::Result<ClientConfig> {
    if let Some(profile_str) = profile {
        Ok(Profile::decode(&profile_str)?)
    } else if let Some(config_path) = config {
        ClientConfig::load_from_file(&config_path).map_err(boxerr)
    } else {
        // Try default config file
        if std::path::Path::new("client.json").exists() {
            ClientConfig::load_from_file("client.json").map_err(boxerr)
        } else {
            Err(anyhow::anyhow!("provide --profile or --config"))
        }
    }
}
