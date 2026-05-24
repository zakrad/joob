# Joob — Complete Setup Guide

This guide walks you through every step from zero to a working tunnel.

**Time required:** ~15 minutes  
**What you need:**
- A VPS with internet access ($3-5/month, any provider)
- A free Google account
- A Windows/Mac/Linux computer (the client)

---

## Table of Contents

1. [Create a Google Cloud Project](#step-1-create-a-google-cloud-project)
2. [Build Joob from Source](#step-2-build-joob-from-source)
3. [Set Up the Exit Node (VPS)](#step-3-set-up-the-exit-node-vps)
4. [Set Up the Client (Your PC)](#step-4-set-up-the-client-your-pc)
5. [Configure Your Browser](#step-5-configure-your-browser)
6. [Verify Everything Works](#step-6-verify-everything-works)
7. [Maintenance & Troubleshooting](#step-7-maintenance--troubleshooting)

---

## Step 1: Create a Google Cloud Project

You need a Google Cloud project with the Drive API enabled and an OAuth client ID. This is free and takes ~5 minutes.

### 1.1 Go to Google Cloud Console

Open [https://console.cloud.google.com](https://console.cloud.google.com) and sign in with your Google account.

### 1.2 Create a New Project

1. Click the project dropdown at the top of the page (it may say "Select a project")
2. Click **"New Project"**
3. Enter a name (e.g. `joob-tunnel`)
4. Click **"Create"**
5. Wait a few seconds, then select your new project from the dropdown

### 1.3 Enable the Google Drive API

1. In the left sidebar, go to **APIs & Services** → **Library**
2. Search for **"Google Drive API"**
3. Click on it, then click **"Enable"**

### 1.4 Configure OAuth Consent Screen

1. Go to **APIs & Services** → **OAuth consent screen**
2. Select **"External"** user type → Click **"Create"**
3. Fill in the required fields:
   - **App name:** `Joob` (or anything you want)
   - **User support email:** your email
   - **Developer contact email:** your email
4. Click **"Save and Continue"**
5. On the **Scopes** page, click **"Add or Remove Scopes"**
   - Search for `drive.file`
   - Check the box for **`../auth/drive.file`** (Google Drive API — See, edit, create, and delete only the specific Google Drive files you use with this app)
   - Click **"Update"**
6. Click **"Save and Continue"**
7. On the **Test users** page, click **"Add Users"**
   - Add your own Google email address
   - Click **"Save and Continue"**
8. Click **"Back to Dashboard"**

### 1.5 Create OAuth Client ID

1. Go to **APIs & Services** → **Credentials**
2. Click **"+ Create Credentials"** → **"OAuth client ID"**
3. Application type: **"Desktop app"** (or "TVs and Limited Input devices")
4. Name: `joob-client` (or anything)
5. Click **"Create"**
6. A dialog appears with your **Client ID** and **Client Secret**
7. **Copy both values** — you'll need them in Step 3

> **Important:** Keep your Client ID and Client Secret private. Anyone with these can use your Google Cloud project's quota.

### 1.6 (Optional) Publish the App

By default, your OAuth app is in "Testing" mode, which limits it to test users you've added and tokens expire after 7 days.

To avoid re-authorizing every week:
1. Go to **OAuth consent screen**
2. Click **"Publish App"**
3. Confirm the warning

> Note: "Publishing" doesn't make your app public or discoverable. It just removes the test-user restriction and token expiry.

---

## Step 2: Build Joob from Source

### Prerequisites

Install the Rust toolchain if you don't have it:

```bash
# Linux / macOS:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Windows (PowerShell):
# Download and run https://rustup.rs
```

Verify:
```bash
rustc --version   # Should be 1.75+
cargo --version
```

### Build

```bash
# Clone the repo (or copy the joob directory to both machines)
cd ~/joob

# Build release binaries
cargo build --release

# Binaries are at:
#   target/release/joob-exit    (~7MB)
#   target/release/joob-client  (~7MB)
```

### Cross-Compile for Windows (from Linux/macOS)

If you're building on Linux/Mac but need a Windows client:

```bash
# Install the Windows target
rustup target add x86_64-pc-windows-gnu

# On Ubuntu/Debian, install the cross-linker:
sudo apt install gcc-mingw-w64-x86-64

# Build for Windows
cargo build --release --target x86_64-pc-windows-gnu -p joob-client

# Binary at: target/x86_64-pc-windows-gnu/release/joob-client.exe
```

### Cross-Compile for Linux (from macOS)

```bash
# Install the Linux target
rustup target add x86_64-unknown-linux-gnu

# You may need a cross-linker — or just build directly on the VPS
cargo build --release --target x86_64-unknown-linux-gnu -p joob-exit
```

> **Tip:** The easiest approach is to build `joob-exit` directly on your VPS and `joob-client` on your local machine.

---

## Step 3: Set Up the Exit Node (VPS)

The exit node runs on a VPS with unrestricted internet access. It receives your tunneled traffic through Google Drive and forwards it to the real internet.

### 3.1 Get a VPS

Any cheap VPS works. Recommended providers:
- [Hetzner](https://hetzner.com) — €3.79/month (Germany)
- [DigitalOcean](https://digitalocean.com) — $4/month
- [Vultr](https://vultr.com) — $3.50/month
- [BuyVM](https://buyvm.net) — $2/month

Requirements:
- Linux (Ubuntu 22.04+ recommended)
- 512MB RAM minimum
- Unrestricted outbound internet

### 3.2 Copy the Binary to Your VPS

```bash
# From your local machine:
scp target/release/joob-exit user@your-vps-ip:~/

# Or build directly on the VPS:
ssh user@your-vps-ip
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
# ... copy source code and build
```

### 3.3 Run the Setup

```bash
ssh user@your-vps-ip

# Make executable
chmod +x joob-exit

# Run interactive setup
./joob-exit setup \
  --client-id "YOUR_CLIENT_ID_FROM_STEP_1" \
  --client-secret "YOUR_CLIENT_SECRET_FROM_STEP_1"
```

This will:
1. Print a Google authorization URL and a code
2. **Open the URL in a browser** on any device (your phone is fine)
3. Sign in with your Google account
4. Enter the code shown in the terminal
5. Approve the Drive file access permission

After authorization succeeds, the setup will:
- Create a `joob-tunnel` folder in your Google Drive
- Generate a 32-byte encryption key
- Save `exit.json` configuration file
- Print a **`joob://...` profile string**

```
╔══════════════════════════════════════════╗
║          SETUP COMPLETE ✓                ║
╠══════════════════════════════════════════╣
║                                          ║
║  Share this profile with the client:     ║
║                                          ║
╚══════════════════════════════════════════╝

joob://eyJ0dW5uZWxfc2VjcmV0Ijoi...

Run the exit node:  joob-exit run
```

> **Copy the `joob://...` string.** This is your client profile — you'll paste it on your PC in Step 4.

### 3.4 Start the Exit Node

```bash
# Foreground (for testing):
./joob-exit run

# Background with nohup:
nohup ./joob-exit run > joob.log 2>&1 &

# Or as a systemd service (recommended):
sudo tee /etc/systemd/system/joob-exit.service > /dev/null <<EOF
[Unit]
Description=Joob Tunnel Exit Node
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$HOME
ExecStart=$HOME/joob-exit run
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable joob-exit
sudo systemctl start joob-exit

# Check status:
sudo systemctl status joob-exit

# View logs:
journalctl -u joob-exit -f
```

---

## Step 4: Set Up the Client (Your PC)

The client runs on your local machine (the one behind the restricted network).

### Windows

1. Copy `joob-client.exe` to any folder (e.g. `C:\joob\`)
2. Open **PowerShell** or **Command Prompt**
3. Navigate to the folder:
   ```powershell
   cd C:\joob
   ```
4. Connect using the profile from Step 3:
   ```powershell
   .\joob-client.exe connect --profile "joob://eyJ0dW5uZWxfc2VjcmV0Ijoi..."
   ```

You should see:
```
╔══════════════════════════════════════════╗
║          JOOB CLIENT                     ║
╠══════════════════════════════════════════╣
║  SOCKS5: 127.0.0.1:1080                 ║
║  HTTP:   127.0.0.1:8080                 ║
╚══════════════════════════════════════════╝
```

### Save the Profile for Later

Instead of pasting the long profile string every time:

```powershell
# Import and save to a config file:
.\joob-client.exe import --profile "joob://eyJ0dW5uZWxfc2VjcmV0Ijoi..." --output client.json

# Next time, just:
.\joob-client.exe connect --config client.json
```

### macOS / Linux

Same commands, just without the `.exe`:
```bash
chmod +x joob-client
./joob-client connect --profile "joob://..."
```

### Custom Ports

```bash
# Use different ports:
./joob-client connect --config client.json --socks-port 9090 --http-port 9091
```

---

## Step 5: Configure Your Browser

With the client running, you need to tell your browser to use the local proxy.

### Option A: Browser Extension (Recommended)

1. Install [Proxy SwitchyOmega](https://chrome.google.com/webstore/detail/proxy-switchyomega) (Chrome/Edge) or [FoxyProxy](https://addons.mozilla.org/en-US/firefox/addon/foxyproxy-standard/) (Firefox)
2. Create a new profile with:
   - **Protocol:** SOCKS5
   - **Server:** `127.0.0.1`
   - **Port:** `1080`
3. Enable the profile
4. Browse any website — traffic goes through the tunnel

### Option B: System-Wide Proxy (Windows)

1. Open **Settings** → **Network & Internet** → **Proxy**
2. Under **Manual proxy setup**, turn on **"Use a proxy server"**
3. Address: `127.0.0.1`
4. Port: `8080`
5. Check **"Don't use the proxy server for local addresses"**
6. Save

### Option C: Firefox Proxy Settings

1. Open Firefox → **Settings** → Search for "proxy"
2. Click **"Settings..."** under Network Settings
3. Select **"Manual proxy configuration"**
4. SOCKS Host: `127.0.0.1`, Port: `1080`
5. Select **"SOCKS v5"**
6. Check **"Proxy DNS when using SOCKS v5"** (important!)
7. Click **OK**

### Option D: Telegram

For Telegram specifically:
- Open Telegram → Settings → Advanced → Connection type
- Select **SOCKS5**
- Server: `127.0.0.1`
- Port: `1080`
- No username/password

Or use this link directly:
```
https://t.me/socks?server=127.0.0.1&port=1080
```

---

## Step 6: Verify Everything Works

### Test 1: Check Your IP

1. Configure your browser to use the proxy (Step 5)
2. Visit [https://httpbin.org/ip](https://httpbin.org/ip)
3. The IP shown should be your **VPS IP**, not your real IP

### Test 2: Command-Line Test

```bash
# Using curl through SOCKS5:
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip

# Using curl through HTTP proxy:
curl --proxy http://127.0.0.1:8080 https://httpbin.org/ip
```

### Test 3: Speed Test

Visit [https://fast.com](https://fast.com) through the proxy to check your throughput.

Expected performance:
- **Latency:** 200-500ms per request
- **Download speed:** 5-20 Mbps (depends on VPS and Google Drive API response time)
- **Monthly capacity:** 500GB+ easily

### Test 4: DNS Leak Test

Visit [https://dnsleaktest.com](https://dnsleaktest.com) through the proxy. DNS should resolve through the VPS, not your local ISP.

> **Important:** Make sure "Proxy DNS when using SOCKS v5" is enabled in your browser.

---

## Step 7: Maintenance & Troubleshooting

### Clean Up Old Drive Files

Over time, rotated tunnel files accumulate in your Drive. Clean them up:

```bash
# On the VPS:
./joob-exit cleanup
```

### Re-Authenticate (Token Expired)

If you see "token refresh failed" errors:

```bash
# On the VPS:
./joob-exit revoke
./joob-exit setup --client-id "YOUR_ID" --client-secret "YOUR_SECRET"

# Then update the client profile
```

### Change the Google Edge IP

If the default Google IP (`216.239.38.120`) doesn't work on your network, try others:

```
216.239.32.120
216.239.34.120
216.239.36.120
216.239.38.120
142.250.185.0 - 142.250.189.255 (Google's range)
```

To find a working IP, try:
```bash
# From the restricted network:
ping www.google.com
# Use whatever IP responds
```

Then update your client config:
```bash
# Edit client.json and change "google_ip"
# Or re-import with the correct IP:
./joob-client import --profile "joob://..." --output client.json
```

### Run as Background Service

#### VPS (Linux systemd) — see Step 3.4

#### Windows (Task Scheduler)

1. Open **Task Scheduler**
2. Create Task → Name: "Joob Client"
3. Trigger: "At log on"
4. Action: Start a program
   - Program: `C:\joob\joob-client.exe`
   - Arguments: `connect --config C:\joob\client.json`
5. Conditions: uncheck "Start only if AC power"
6. Settings: check "If task fails, restart every 1 minute"

### Multiple Google Accounts (Higher Throughput)

For heavy usage, you can pool multiple Google accounts:

1. Run `joob-exit setup` with each Google account
2. Each setup creates a separate `exit.json` and profile
3. Use the same tunnel secret across all accounts
4. (Advanced: edit config to add multiple oauth entries)

### Common Issues

| Problem | Cause | Fix |
|---------|-------|-----|
| "failed to upload data" | Drive API rate limit | Wait 1 min, or add more Google accounts |
| "token refresh failed" | Token expired/revoked | Run `joob-exit revoke` then `setup` |
| Slow browsing | High latency to Google | Try a different Google edge IP |
| Can't reach Google at all | Network fully blocked | Joob can't help if Google itself is blocked |
| "connection refused" on 1080 | Client not running | Start `joob-client connect` first |
| Sites show CAPTCHA | Target sees Google/VPS IP | Normal for Cloudflare-protected sites |
| YouTube doesn't stream | Bandwidth too high | Lower video quality, or try different time of day |

---

## Security Notes

- **The VPS sees your traffic** — it dials websites on your behalf, just like any VPN/proxy exit. Use HTTPS sites to keep content private from the VPS operator.
- **Google sees Drive API calls** — file creates/reads/deletes from your Google account. The file contents are encrypted (AES-256-GCM), so Google cannot read the tunnel data.
- **The `joob://` profile contains secrets** — your tunnel encryption key and OAuth refresh token. Treat it like a password. Don't share it publicly.
- **The `exit.json` file contains secrets** — same as above. Keep it secure on your VPS.

---

## Architecture Reference

```
┌─────────────────────────────────────────────────────────┐
│                    CLIENT SIDE                           │
│                                                          │
│  Browser → SOCKS5/HTTP Proxy (127.0.0.1:1080/8080)      │
│              ↓                                           │
│  Mux: TCP connections → multiplexed frames               │
│              ↓                                           │
│  Sender: batch frames → encrypt (AES-256-GCM) → upload  │
│              ↓                                           │
│  Drive Upload: create/update up_NNN.bin files            │
│              ↓                                           │
│  TLS to Google IP (SNI=googleapis, looks like Drive sync)│
│                                                          │
├─────────────────── GOOGLE DRIVE ─────────────────────────┤
│                                                          │
│  Shared folder: joob-tunnel/                             │
│    up_000001.bin, up_000002.bin, ...  (client → exit)    │
│    dn_000001.bin, dn_000002.bin, ...  (exit → client)    │
│                                                          │
├─────────────────────────────────────────────────────────-─┤
│                    EXIT SIDE (VPS)                        │
│                                                          │
│  Drive Download: Range GET on up_NNN.bin (new bytes only)│
│              ↓                                           │
│  Receiver: decrypt → parse frames → dispatch to mux      │
│              ↓                                           │
│  Mux: OPEN frame → TCP connect to target                 │
│              ↓                                           │
│  Target website ← TCP → response data                    │
│              ↓                                           │
│  Sender: batch response frames → encrypt → upload dn_*   │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

---

## File Structure

```
~/                    (or C:\joob\ on Windows)
├── joob-exit         (VPS binary)
├── joob-client       (client binary)
├── exit.json         (VPS config — contains secrets)
├── exit_token.json   (VPS OAuth token — contains secrets)
├── client.json       (client config — contains secrets)
└── joob.log          (VPS log file, if using nohup)
```

Files in your Google Drive:
```
My Drive/
└── joob-tunnel/
    ├── up_000001.bin   (encrypted upstream data)
    ├── up_000002.bin
    ├── dn_000001.bin   (encrypted downstream data)
    └── dn_000002.bin
```

These files rotate automatically. Run `joob-exit cleanup` periodically to remove old ones.
