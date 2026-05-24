# Joob (جوب)

*Data flows through Google's infrastructure like water through a joob — always there, nobody notices.*

A high-throughput TCP tunnel that routes all traffic through Google Drive API. From the network's perspective, you're just syncing files to Google Drive.

## How It Works

```
Client (restricted network)           VPS Exit (free internet)
┌──────────────┐                     ┌──────────────┐
│  joob-client │                     │  joob-server  │
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

> **Full step-by-step guide: [docs/SETUP.md](docs/SETUP.md)**

## Quick Start

### Prerequisites: Google OAuth (one-time, 5 min)

1. Open [console.cloud.google.com](https://console.cloud.google.com) → create a project
2. Enable **Google Drive API**
3. Create **OAuth consent screen** (External) → publish the app
4. Create **OAuth client ID** → type: **Desktop app** → copy the **Client ID**

> No client secret needed — Joob uses PKCE (Proof Key for Code Exchange).
>
> Detailed instructions in [docs/SETUP.md](docs/SETUP.md#step-1-google-cloud-oauth-setup-one-time-5-minutes)

### 1. VPS Setup (exit node)

SSH into any Linux VPS and run:

```bash
curl -sSL https://raw.githubusercontent.com/zakrad/joob/master/scripts/install-exit.sh | sudo bash
```

This downloads a pre-built binary (~30 seconds, no compilation needed).

Then run the setup wizard:

```bash
cd ~/joob && joob-server setup
```

It will:
1. Ask for your Client ID
2. Print a Google authorization URL — open it in any browser, authorize, then paste the redirect URL back
3. Create a Drive folder and generate a `joob://...` profile string — **copy it**

Start the service:
```bash
systemctl start joob
```

### 2. Client Setup

#### Windows (GUI)

Download `joob-windows-amd64.exe` from [Releases](https://github.com/zakrad/joob/releases).

Run it, paste the `joob://...` profile, click **Connect**.

#### Windows / macOS / Linux (CLI)

Download the binary for your platform from [Releases](https://github.com/zakrad/joob/releases), or build from source:

```bash
git clone https://github.com/zakrad/joob.git && cd joob
cargo build --release
```

Then:

```bash
# Save the profile:
joob-client import --profile "joob://..." --output client.json

# Connect:
joob-client connect --config client.json
```

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

- **Pre-built binaries** — no Rust required, just download and run
- **Desktop GUI** — Windows app with one-click connect
- **PKCE OAuth** — no client secret needed, just a Client ID
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

## Downloads

Pre-built binaries for every release:

| Platform | Binary | Description |
|----------|--------|-------------|
| Linux x64 | `joob-server-linux-amd64` | Server for VPS |
| Linux ARM64 | `joob-server-linux-arm64` | Server for ARM VPS (Oracle, RPi) |
| Linux x64 | `joob-client-linux-amd64` | CLI client |
| Linux ARM64 | `joob-client-linux-arm64` | CLI client |
| Windows x64 | `joob-windows-amd64.exe` | Desktop GUI app |
| Windows x64 | `joob-client-windows-amd64.exe` | CLI client |

Download from [Releases](https://github.com/zakrad/joob/releases).

## VPS Management

```bash
# Check status:
systemctl status joob

# View logs:
journalctl -u joob --no-pager -n 50

# Clean up old Drive files (do weekly):
joob-server cleanup

# Restart:
systemctl restart joob
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "Token refresh failed" | Run `joob-server revoke` then `joob-server setup` |
| Slow speed | Normal — Drive API has latency. Lower video quality |
| "Rate limited" | Wait 1 min, auto-recovers |
| Connection drops | `systemctl restart joob` on VPS |
| IP check shows real IP | Check browser proxy settings |
| Google completely blocked | Joob requires Google access to work |

## Building from Source

```bash
# Requires Rust 1.75+
cargo build --release

# Binaries in target/release/
# joob-server, joob-client, joob (GUI)
```

## Disclaimer

Joob is provided for educational and research purposes. You are responsible for compliance with applicable laws and service terms. See DISCLAIMER.md.

## License

MIT
