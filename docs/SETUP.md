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
| Rust | **No** | — | Pre-built binaries are available |

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

You run `joob-exit` on a VPS, `joob-client` (or the GUI app) on your PC. All traffic looks like Google Drive file syncing to anyone watching your network.

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
3. Application type: **"Desktop app"**
4. Name: anything
5. Click **"Create"**
6. A popup shows your **Client ID** — copy it

> **No client secret needed!** Joob uses PKCE (Proof Key for Code Exchange),
> which doesn't require a secret. You only need the Client ID.

That's it for Google. You now have a `Client ID`.

---

## Step 2: VPS Setup (exit node)

### 2.1 — Get a VPS

Buy the cheapest Linux VPS you can find. Any provider works:

| Provider | Price | How to get |
|----------|-------|-----------|
| [Hetzner](https://hetzner.com/cloud) | €3.79/mo | Sign up → Cloud → Add Server → Ubuntu 22.04 |
| [DigitalOcean](https://digitalocean.com) | $4/mo | Sign up → Create Droplet → Ubuntu |
| [Vultr](https://vultr.com) | $3.50/mo | Sign up → Deploy Server → Ubuntu |
| [BuyVM](https://buyvm.net) | $2/mo | Cheapest option |

Requirements: Linux, 512MB RAM, internet access. That's it.

### 2.2 — One-Line Install

SSH into your VPS and run:

```bash
curl -sSL https://raw.githubusercontent.com/zakrad/joob/master/scripts/install-exit.sh | sudo bash
```

This downloads a pre-built binary (~30 seconds). **No Rust, no compilation, no build tools.**

### 2.3 — Run Setup

```bash
cd ~/joob
joob-exit setup
```

It will:
1. Ask for your **Client ID** (from Step 1)
2. Print a Google authorization URL
3. You open the URL in any browser (phone, other PC — anything)
4. Authorize the app, then paste the redirected URL back into the terminal
5. Create a Drive folder and generate config

At the end it prints a `joob://...` profile string — **copy it**.

### 2.4 — Start the Exit Node

```bash
systemctl start joob
```

Check it's running:
```bash
systemctl status joob
```

**VPS setup is done.** You can close the SSH session.

---

## Step 3: Client Setup (your PC)

### Windows (GUI — easiest)

1. Download `joob-windows-amd64.exe` from [Releases](https://github.com/zakrad/joob/releases)
2. Run it
3. Paste the `joob://...` profile string
4. Click **Import Profile**
5. Click **Connect**

### Windows (CLI)

1. Download `joob-client-windows-amd64.exe` from [Releases](https://github.com/zakrad/joob/releases)
2. Open PowerShell:

```powershell
cd C:\Users\YOU\Downloads

# Save the profile:
.\joob-client-windows-amd64.exe import --profile "joob://..." --output client.json

# Connect:
.\joob-client-windows-amd64.exe connect --config client.json
```

### macOS / Linux (CLI)

Download from [Releases](https://github.com/zakrad/joob/releases) or build from source:

```bash
# Download:
curl -fsSL https://github.com/zakrad/joob/releases/latest/download/joob-client-linux-amd64 -o joob-client
chmod +x joob-client

# Save the profile:
./joob-client import --profile "joob://..." --output client.json

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
```

**Leave this running** while you browse.

---

## Step 4: Tell Your Browser to Use the Tunnel

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

## Step 5: Verify It Works

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
ssh root@VPS_IP "joob-exit cleanup"

# Restart:
ssh root@VPS_IP "systemctl restart joob"
```

### Alternative OAuth Flow

If the default PKCE flow doesn't work, you can use the device-code flow instead:

1. In Google Cloud Console, create a **"TVs and Limited Input devices"** client (instead of Desktop)
2. Copy both the **Client ID** and **Client Secret**
3. Run setup with:

```bash
joob-exit setup --client-id "YOUR_ID" --client-secret "YOUR_SECRET" --oauth-flow device
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| "token refresh failed" | Token expired or revoked | On VPS: `joob-exit revoke` then `joob-exit setup` |
| Can't reach `httpbin.org/ip` through proxy | Client not connected | Make sure `joob-client connect` is running |
| IP check shows your real IP | Browser not using proxy | Check proxy settings (Step 4) |
| Very slow browsing | Normal — Drive API has latency | Try off-peak hours, or lower video quality |
| "rate limited" in logs | Too many Drive API calls | Wait 1 min, reduces automatically |
| Connection drops after hours | VPS restart or token issue | `systemctl restart joob` on VPS |
| Google is completely blocked | Network blocks ALL Google | Joob can't help — it needs Google access |

---

## Security Notes

- **Your VPS can see what websites you visit** (like any proxy). Use HTTPS.
- **OAuth tokens** are stored locally on disk. Protect your config files.
- **Tunnel data** is AES-256-GCM encrypted — unreadable even to Google.
- **Google can see** you're uploading/downloading files, but not what's in them.
