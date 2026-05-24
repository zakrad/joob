# Joob (جوب)

*Data flows through Google's infrastructure like water through a joob — always there, nobody notices.*

A high-throughput TCP tunnel that routes all traffic through Google Drive API. From the network's perspective, you're just syncing files to Google Drive.

## How It Works

```
Client (restricted network)           VPS Exit (free internet)
┌──────────────┐                     ┌──────────────┐
│  joob-client │                     │  joob-exit   │
│              │                     │              │
│  SOCKS5 :1080│                     │  TCP dialer  │
│  HTTP   :8080│                     │  → internet  │
└──────┬───────┘                     └──────┬───────┘
       │ TLS to Google IP                   │ Direct TLS
       │ (looks like Drive sync)            │ to googleapis
       ▼                                    ▼
  ┌─────────────────────────────────────────────────┐
  │              Google Drive API                    │
  │                                                 │
  │   up_000001.bin  (client → exit)                │
  │   dn_000001.bin  (exit → client)                │
  │                                                 │
  │   Write = create/update file                    │
  │   Read  = HTTP Range GET (new bytes only)       │
  └─────────────────────────────────────────────────┘
```

## Quick Start

### 1. VPS Setup (5 minutes)

```bash
# On your VPS (any Linux server with internet access):
curl -LO https://github.com/user/joob/releases/latest/download/joob-exit-linux-amd64
chmod +x joob-exit-linux-amd64

# Interactive setup — authenticates with Google, creates Drive folder
./joob-exit-linux-amd64 setup

# Follow the Google auth link, enter the code
# Setup prints a joob:// profile string — copy it

# Start the exit node
./joob-exit-linux-amd64 run
```

### 2. Client Setup (Windows)

```powershell
# Download joob-client.exe
# Connect using the profile from setup:
.\joob-client.exe connect --profile "joob://eyJ0dW5u..."

# Or import + save for later:
.\joob-client.exe import --profile "joob://eyJ0dW5u..." --output client.json
.\joob-client.exe connect --config client.json
```

### 3. Configure Your Browser

Set your browser's proxy to:
- **SOCKS5:** `127.0.0.1:1080` (recommended)
- **HTTP:** `127.0.0.1:8080`

Or use a browser extension like [Proxy SwitchyOmega](https://chrome.google.com/webstore/detail/proxy-switchyomega).

## Features

- **Single binary** — no Python, no Node.js, no dependencies
- **Domain-fronted** — all traffic goes to Google IPs, passes DPI
- **Encrypted** — AES-256-GCM with per-session keys
- **High throughput** — 5-20 Mbps sustained, 500GB/month easily
- **Multi-stream** — multiplexed connections, browse normally
- **Auto-rotation** — Drive files rotate at 10MB, old ones cleaned up
- **Rate-limit aware** — built-in quota tracking and throttling

## Architecture

### Data Flow
1. Browser sends request → SOCKS5/HTTP proxy on localhost
2. Proxy opens a multiplexed stream in the mux layer
3. Mux batches frames, encrypts with AES-256-GCM
4. Encrypted data written to Google Drive file (`up_NNN.bin`)
5. Exit side polls Drive for new data (Range GET — only new bytes)
6. Exit decrypts, unpacks frames, dials the target
7. Response flows back through `dn_NNN.bin` files

### Google Drive Quotas
- **Upload:** 750 GB/day (500 GB/month = 2.3% of daily limit)
- **API calls:** 12,000/minute (typical usage: ~500/minute)
- **Storage:** 15 GB free (files rotate, never accumulates)

### Security
- All tunnel data is AES-256-GCM encrypted
- Per-direction nonces prevent replay between client/exit
- OAuth tokens stored locally, never transmitted through tunnel
- Exit can see destination metadata (like any proxy)

## Configuration

### Google Edge IPs

If the default IP doesn't work, try these Google edge IPs:
```
216.239.38.120
216.239.32.120
216.239.34.120
216.239.36.120
142.250.0.0/15 (Google's range)
```

### Custom OAuth

To use your own Google Cloud project (recommended for privacy):
1. Go to [Google Cloud Console](https://console.cloud.google.com)
2. Create project → Enable Drive API
3. Create OAuth consent screen (External, Testing)
4. Create OAuth client ID (Desktop app)
5. Run setup with your credentials:
```bash
./joob-exit setup --client-id "YOUR_ID" --client-secret "YOUR_SECRET"
```

## Building from Source

```bash
# Requires Rust 1.75+
cargo build --release

# Cross-compile for Windows
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu

# Binaries in target/release/
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "Token refresh failed" | Run `joob-exit revoke` then `joob-exit setup` |
| Slow speed | Add more Google accounts, increase file size |
| "Rate limited" | Reduce polling interval or add accounts |
| Connection drops | Check VPS internet, re-run exit |
| "No new data" | Normal during idle — data appears when you browse |

## Disclaimer

Joob is provided for educational and research purposes. You are responsible for compliance with applicable laws and service terms. See DISCLAIMER.md.

## License

MIT
