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
  ┌─────────────────────────────────────────────────────┐
  │              Google Drive API                        │
  │                                                     │
  │   up_000001.bin  (client → exit)                    │
  │   dn_000001.bin  (exit → client)                    │
  │                                                     │
  │   Write = create/update file                        │
  │   Read  = HTTP Range GET (new bytes only)           │
  └─────────────────────────────────────────────────────┘
```

## What You Need

| Requirement | Cost | Notes |
|------------|------|-------|
| Google account | **Free** | Any Gmail. This is the tunnel medium |
| Google Cloud project | **Free** | OAuth app setup, no billing/credit card needed |
| VPS | **$2-5/month** | Any Linux server — the only paid thing |
| Domain / DNS | **Not needed** | — |
| Rust toolchain | **Free** | To build from source |

> **Full step-by-step guide: [docs/SETUP.md](docs/SETUP.md)**

## Quick Start

### Prerequisites: Google OAuth (one-time, 5 min)

1. Open [console.cloud.google.com](https://console.cloud.google.com) → create a project
2. Enable **Google Drive API**
3. Create **OAuth consent screen** (External) → publish the app
4. Create **OAuth client ID** (Desktop app) → copy the **Client ID** and **Client Secret**

> Detailed instructions with screenshots in [docs/SETUP.md](docs/SETUP.md#step-1-google-cloud-oauth-setup-one-time-5-minutes)

### 1. VPS Setup (exit node)

SSH into any Linux VPS and run:

```bash
# One-line install (installs Rust, builds, sets up systemd auto-start):
curl -sSL https://raw.githubusercontent.com/zakrad/joob/master/scripts/install-exit.sh | sudo bash

# Run the setup wizard (authenticates with Google, creates Drive folder):
~/joob-exit setup --client-id "YOUR_CLIENT_ID" --client-secret "YOUR_CLIENT_SECRET"

# Start the exit node:
systemctl start joob
```

The setup wizard will show a URL + code. Open the URL on any device, enter the code, and approve. After that it prints a `joob://...` profile string — **copy it**.

### 2. Client Setup (your PC)

```bash
# Build on your machine:
git clone https://github.com/zakrad/joob.git && cd joob
cargo build --release

# Save the profile:
./target/release/joob-client import --profile "joob://..." --output client.json

# Connect:
./target/release/joob-client connect --config client.json
```

**Windows:** same but use `joob-client.exe` after cross-compiling or building on Windows.

### 3. Configure Your Browser

Set your browser proxy to **SOCKS5 `127.0.0.1:1080`** using:
- [Proxy SwitchyOmega](https://chrome.google.com/webstore/detail/proxy-switchyomega) (Chrome/Edge)
- [FoxyProxy](https://addons.mozilla.org/en-US/firefox/addon/foxyproxy-standard/) (Firefox)
- Or Firefox → Settings → search "proxy" → Manual → SOCKS5 `127.0.0.1:1080`

### 4. Verify

```bash
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip
# Should show your VPS IP, not your real IP
```

## Features

- **Single binary** — no Python, no Node.js, no dependencies
- **Domain-fronted** — all traffic goes to Google IPs, passes DPI
- **Encrypted** — AES-256-GCM with per-session keys
- **High throughput** — 5-20 Mbps sustained, 500GB/month easily
- **Multi-stream** — multiplexed connections, browse normally
- **Auto-rotation** — Drive files rotate at 10MB, old ones cleaned up
- **Rate-limit aware** — built-in quota tracking and throttling
- **Auto-start** — systemd service, restarts on crash

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
- **Upload:** 750 GB/day (typical usage: fraction of this)
- **API calls:** 12,000/minute (typical usage: ~500/minute)
- **Storage:** 15 GB free (files rotate, never accumulates)

### Security
- All tunnel data is AES-256-GCM encrypted
- Per-direction nonces prevent replay between client/exit
- OAuth tokens stored locally, never transmitted through tunnel
- Exit can see destination metadata (like any proxy)

## VPS Management

```bash
# Check status:
systemctl status joob

# View logs:
journalctl -u joob --no-pager -n 50

# Clean up old Drive files (do weekly):
~/joob-exit cleanup

# Restart:
systemctl restart joob
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "Token refresh failed" | Run `joob-exit revoke` then `joob-exit setup ...` |
| Slow speed | Normal — Drive API has latency. Lower video quality |
| "Rate limited" | Wait 1 min, auto-recovers |
| Connection drops | `systemctl restart joob` on VPS |
| IP check shows real IP | Check browser proxy settings |
| Google completely blocked | Joob requires Google access to work |

## Building from Source

```bash
# Requires Rust 1.75+
cargo build --release

# Cross-compile for Windows:
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu -p joob-client

# Binaries in target/release/
```

## Disclaimer

Joob is provided for educational and research purposes. You are responsible for compliance with applicable laws and service terms. See DISCLAIMER.md.

## License

MIT
