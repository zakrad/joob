# Joob — Setup Guide

## What You Need (and What You Don't)

| Requirement | Needed? | Cost | Notes |
|------------|---------|------|-------|
| Google account | **Yes** | Free | Any Gmail works. This is the tunnel medium. |
| Google Cloud project | **Yes** | Free | You create an OAuth app (5 min, no credit card) |
| Domain / DNS | **No** | — | Not needed at all |
| VPS (server) | **Yes** | $2-5/month | Any Linux server with internet. The "exit door" |
| Credit card for Google | **No** | — | Google Cloud project is free, no billing required |
| Static IP | **No** | — | VPS comes with one. Client doesn't need one |
| Port forwarding | **No** | — | Nothing listens on public ports |
| Rust (programming language) | **Yes** | Free | To build from source. Takes 2 minutes to install |

**Total cost: $0-5/month** (only the VPS).

---

## Overview: What Happens

```
YOUR PC (restricted network)        YOUR VPS (free internet)
  Browser                             joob-exit
    ↓                                   ↓
  joob-client                      dials websites
  (local proxy)                    on your behalf
    ↓                                   ↑
    └───── Google Drive files ──────────┘
          (encrypted, looks like
           normal Google Drive sync)
```

You run `joob-exit` on a VPS, `joob-client` on your PC. All traffic looks like Google Drive file syncing to anyone watching your network.

---

## Step 1: Google Cloud OAuth Setup (one-time, 5 minutes)

This creates a free "app" that lets Joob access a folder in your Google Drive. No credit card, no billing.

### 1.1 — Create a Google Cloud Project

1. Open **[console.cloud.google.com](https://console.cloud.google.com)**
2. Sign in with any Google/Gmail account
3. If it asks to agree to Terms of Service, accept them
4. At the top bar, click the project dropdown → **"New Project"**
5. Name it anything (e.g. `my-project`) → **"Create"**
6. Make sure your new project is selected in the top dropdown

### 1.2 — Enable Drive API

1. In the left menu: **APIs & Services** → **Library**
2. Search: `Google Drive API`
3. Click it → click **"Enable"**

### 1.3 — OAuth Consent Screen

1. Left menu: **APIs & Services** → **OAuth consent screen**
2. Click **"Get started"** (or "Configure consent screen")
3. App name: type anything (e.g. `myapp`)
4. User support email: select your email
5. Audience: select **External**
6. Contact email: type your email
7. Click **"Create"** or **"Save"**
8. On the next screens keep clicking **Continue / Save** (defaults are fine)
9. When you see **"Publish App"** button, click it and confirm
   - This is NOT public — it just removes the 7-day token expiry

### 1.4 — Create Client ID

1. Left menu: **APIs & Services** → **Credentials**
2. Click **"+ Create Credentials"** → **"OAuth client ID"**
3. Application type: **"Desktop app"** (or "TVs and Limited Input devices")
4. Name: anything
5. Click **"Create"**
6. A popup shows your **Client ID** and **Client Secret**
7. **Copy both** — you'll paste them in Step 3

That's it for Google. You now have a `Client ID` and `Client Secret`.

---

## Step 2: Build Joob (2 minutes)

### Install Rust (if you don't have it)

```bash
# Linux / macOS (one command):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
```

### Build

```bash
cd ~/joob
cargo build --release
```

This produces two binaries:
- `target/release/joob-exit` — for the VPS (~7MB)
- `target/release/joob-client` — for your PC (~7MB)

### Building for Windows (from Linux/Mac)

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64   # Ubuntu/Debian only
cargo build --release --target x86_64-pc-windows-gnu -p joob-client
# Output: target/x86_64-pc-windows-gnu/release/joob-client.exe
```

---

## Step 3: VPS Setup (the exit node)

### 3.1 — Get a VPS

Buy the cheapest Linux VPS you can find. Any provider works:

| Provider | Price | How to get |
|----------|-------|-----------|
| [Hetzner](https://hetzner.com/cloud) | €3.79/mo | Sign up → Cloud → Add Server → Ubuntu 22.04 |
| [DigitalOcean](https://digitalocean.com) | $4/mo | Sign up → Create Droplet → Ubuntu |
| [Vultr](https://vultr.com) | $3.50/mo | Sign up → Deploy Server → Ubuntu |
| [BuyVM](https://buyvm.net) | $2/mo | Cheapest option |

Requirements: Linux, 512MB RAM, internet access. That's it.

After you get the VPS, you'll have an **IP address** and **SSH credentials**.

### 3.2 — Copy the Binary to Your VPS

Build on your local machine first (Step 2), then copy the binary:

```bash
# From your LOCAL machine (not the VPS):
scp ~/joob/target/release/joob-exit root@YOUR_VPS_IP:~/joob-exit
```

**OR** build directly on the VPS if you prefer:

```bash
ssh root@YOUR_VPS_IP
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
# copy the joob source to the VPS, then:
cd ~/joob && cargo build --release
cp target/release/joob-exit ~/joob-exit
```

### 3.3 — Run Setup

On the VPS:

```bash
chmod +x ~/joob-exit

~/joob-exit setup \
  --client-id "PASTE_YOUR_CLIENT_ID_HERE" \
  --client-secret "PASTE_YOUR_CLIENT_SECRET_HERE"
```

**What happens:**

```
╔══════════════════════════════════════════╗
║          JOOB EXIT SETUP                 ║
╚══════════════════════════════════════════╝

Step 1/4: Google authentication...

╔══════════════════════════════════════════╗
║         GOOGLE AUTHORIZATION             ║
╠══════════════════════════════════════════╣
║                                          ║
║  Visit: https://www.google.com/device    ║
║  Code:  ABCD-EFGH                        ║
║                                          ║
╚══════════════════════════════════════════╝
```

1. **Open the URL** shown (on any device — your phone, another PC, anything)
2. **Enter the code** shown (e.g. `ABCD-EFGH`)
3. Sign in with the same Google account from Step 1
4. Click **"Allow"** when it asks for Drive access

After you approve, the setup finishes automatically:

```
Step 2/4: Creating Drive folder...
Step 3/4: Generating tunnel secret...
Step 4/4: Config saved to exit.json

╔══════════════════════════════════════════╗
║          SETUP COMPLETE ✓                ║
╚══════════════════════════════════════════╝

joob://eyJ0dW5uZWxfc2VjcmV0IjoiYWJjZGVmZy4uLi4uLi4u...
```

5. **Copy the `joob://...` line** — this is your client connection string

### 3.4 — Start the Exit Node

Quick start (stays running while your SSH is open):
```bash
~/joob-exit run
```

Run in background (keeps running after you disconnect):
```bash
nohup ~/joob-exit run > ~/joob.log 2>&1 &
```

**Recommended: auto-start on boot** — run this once:
```bash
cat > /etc/systemd/system/joob.service << 'EOF'
[Unit]
Description=Joob Exit Node
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/root
ExecStart=/root/joob-exit run
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable joob
systemctl start joob
```

Check it's running:
```bash
systemctl status joob
```

**VPS setup is done.** You can close the SSH session.

---

## Step 4: Client Setup (your PC)

### Windows

1. Copy `joob-client.exe` to a folder (e.g. `C:\joob\`)
2. Open PowerShell or Command Prompt
3. Run:

```powershell
cd C:\joob

# First time — save the profile:
.\joob-client.exe import --profile "joob://eyJ0dW5uZWxfc2Vj..." --output client.json

# Connect:
.\joob-client.exe connect --config client.json
```

### macOS / Linux

```bash
chmod +x joob-client

# First time — save the profile:
./joob-client import --profile "joob://eyJ0dW5uZWxfc2Vj..." --output client.json

# Connect:
./joob-client connect --config client.json
```

### What You Should See

```
╔══════════════════════════════════════════╗
║          JOOB CLIENT                     ║
╠══════════════════════════════════════════╣
║  SOCKS5: 127.0.0.1:1080                 ║
║  HTTP:   127.0.0.1:8080                 ║
╚══════════════════════════════════════════╝

Joob client tunnel running. Press Ctrl+C to stop.
```

**Leave this running** while you browse.

---

## Step 5: Tell Your Browser to Use the Tunnel

Pick ONE of these methods:

### Method A — Browser Extension (easiest, recommended)

**Chrome / Edge:**
1. Install [Proxy SwitchyOmega](https://chrome.google.com/webstore/detail/proxy-switchyomega)
2. Click the extension icon → **"Options"**
3. Click **"New profile"** → name it `Joob` → type **"Proxy Profile"** → **"Create"**
4. Set:
   - Protocol: **SOCKS5**
   - Server: `127.0.0.1`
   - Port: `1080`
5. Click **"Apply changes"** (left sidebar)
6. Click the extension icon → select **"Joob"**

**Firefox:**
1. Install [FoxyProxy Standard](https://addons.mozilla.org/en-US/firefox/addon/foxyproxy-standard/)
2. Click the icon → **"Options"**
3. Add new proxy: SOCKS5, `127.0.0.1`, port `1080`
4. Enable it

### Method B — Firefox Built-in Settings

1. Firefox → **Settings** → search `proxy`
2. Click **"Settings..."**
3. Select **"Manual proxy configuration"**
4. SOCKS Host: `127.0.0.1` — Port: `1080`
5. Select **"SOCKS v5"**
6. ✅ Check **"Proxy DNS when using SOCKS v5"** ← important!
7. Click **"OK"**

### Method C — Windows System Proxy

1. Windows Settings → **Network & Internet** → **Proxy**
2. Under Manual proxy setup: turn **ON**
3. Address: `127.0.0.1`
4. Port: `8080`
5. Save

### Method D — Telegram Only

Telegram → Settings → Advanced → Connection type → **Custom** (SOCKS5)
- Server: `127.0.0.1`
- Port: `1080`
- No username/password

---

## Step 6: Verify It Works

### Check your IP

1. Open your proxied browser
2. Go to **[https://httpbin.org/ip](https://httpbin.org/ip)**
3. It should show your **VPS IP**, NOT your real IP

### Command line check

```bash
# Through the SOCKS proxy:
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip

# Through the HTTP proxy:
curl -x http://127.0.0.1:8080 https://httpbin.org/ip
```

If it shows your VPS IP → **everything works!**

### DNS leak check

Go to [https://dnsleaktest.com](https://dnsleaktest.com) through the proxy → click "Extended test". You should only see your VPS provider's DNS, not your local ISP.

---

## Quick Reference

### Daily Usage

```bash
# Start (leave running):
./joob-client connect --config client.json

# Stop:
Ctrl+C
```

### VPS Management

```bash
# Check exit node status:
ssh root@VPS_IP "systemctl status joob"

# View logs:
ssh root@VPS_IP "journalctl -u joob --no-pager -n 50"

# Clean up old Drive files (do this weekly):
ssh root@VPS_IP "~/joob-exit cleanup"

# Restart:
ssh root@VPS_IP "systemctl restart joob"
```

### If Google IP Doesn't Work

The client connects to Google through a specific IP. If the default doesn't work, try alternatives:

```
216.239.32.120
216.239.34.120
216.239.36.120
216.239.38.120
```

Find a working one:
```bash
# From your restricted network:
ping www.google.com
# Use whatever IP responds
```

Edit `client.json` and change the `"google_ip"` field.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `joob-exit setup` shows "PLACEHOLDER" error | Missing OAuth credentials | Add `--client-id` and `--client-secret` flags |
| "token refresh failed" | Token expired or revoked | On VPS: `~/joob-exit revoke` then `~/joob-exit setup ...` |
| Can't reach `httpbin.org/ip` through proxy | Client not connected | Make sure `joob-client connect` is running |
| IP check shows your real IP | Browser not using proxy | Check proxy settings (Step 5) |
| Very slow browsing | Normal — Drive API has latency | Try off-peak hours, or lower video quality |
| "rate limited" in logs | Too many Drive API calls | Wait 1 min, reduces automatically |
| Connection drops after hours | VPS restart or token issue | `systemctl restart joob` on VPS |
| "failed to upload/download" | Drive API temporary error | Auto-retries, wait a few seconds |
| Google is completely blocked | Network blocks ALL Google | Joob can't help — it needs Google access |

---

## How It Works (Technical)

1. `joob-client` opens a SOCKS5 proxy on your PC at `127.0.0.1:1080`
2. When your browser makes a request, the client encrypts it (AES-256-GCM)
3. Encrypted data is uploaded as a file to Google Drive (`up_000001.bin`)
4. `joob-exit` on the VPS polls Google Drive, sees the new file
5. Exit downloads it, decrypts it, connects to the real website
6. Response is encrypted and uploaded to Drive as `dn_000001.bin`
7. Client downloads it, decrypts, sends back to your browser
8. Files rotate at 10MB and old ones are cleaned up

**What an observer sees:** HTTPS connections to Google IPs. Normal Google Drive traffic. The file contents are AES-256-GCM encrypted — unreadable even to Google.

---

## Security Notes

- **Your VPS can see what websites you visit** (like any proxy/VPN). Use HTTPS sites.
- **Google can see you're making Drive API calls** but cannot read the encrypted file contents.
- **Your `joob://` profile and `client.json` contain secrets.** Don't share them publicly.
- **The `exit.json` and `exit_token.json` on VPS contain secrets.** Secure your VPS.
- **No domain or DNS is ever needed or used.** Zero DNS footprint.
