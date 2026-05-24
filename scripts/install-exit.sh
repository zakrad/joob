#!/usr/bin/env bash
# Joob Exit Node — one-line installer
# Usage: curl -sSL https://raw.githubusercontent.com/zakrad/joob/master/scripts/install-exit.sh | bash
set -euo pipefail

REPO="zakrad/joob"
BINARY="joob-exit-linux-amd64"
INSTALL_DIR="/usr/local/bin"
SERVICE_NAME="joob"
WORK_DIR="$HOME/joob"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[*]${NC} $1"; }
warn()  { echo -e "${YELLOW}[!]${NC} $1"; }
error() { echo -e "${RED}[✗]${NC} $1"; exit 1; }

# ── Checks ──────────────────────────────────────────────────────────
[[ $(id -u) -eq 0 ]] || error "Run as root: curl -sSL ... | sudo bash"
command -v curl &>/dev/null || error "curl is required"

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║     JOOB EXIT NODE — QUICK INSTALL       ║"
echo "╚══════════════════════════════════════════╝"
echo ""

# ── Step 1: Get latest release ──────────────────────────────────────
info "[1/4] Finding latest release..."

LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')

if [[ -z "$LATEST" ]]; then
    warn "Could not find latest release, using 'master' branch."
    DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${BINARY}"
else
    info "  Latest version: ${LATEST}"
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST}/${BINARY}"
fi

# ── Step 2: Download binary ─────────────────────────────────────────
info "[2/4] Downloading joob-exit..."

curl -fsSL -o /tmp/joob-exit "$DOWNLOAD_URL" || error "Download failed. Is there a release at ${DOWNLOAD_URL}?"
chmod +x /tmp/joob-exit
mv /tmp/joob-exit "${INSTALL_DIR}/joob-exit"
info "  Installed to ${INSTALL_DIR}/joob-exit"

# ── Step 3: Create working directory ────────────────────────────────
info "[3/4] Setting up working directory..."
mkdir -p "$WORK_DIR"

# ── Step 4: Create systemd service ──────────────────────────────────
info "[4/4] Installing systemd service..."

cat > /etc/systemd/system/${SERVICE_NAME}.service <<EOF
[Unit]
Description=Joob Exit Tunnel
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=${WORK_DIR}
ExecStart=${INSTALL_DIR}/joob-exit run --config exit.json
Restart=always
RestartSec=5
LimitNOFILE=65536
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable ${SERVICE_NAME}

# ── Step 5: Run setup ───────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════╗"
echo "║        INSTALL COMPLETE ✓                 ║"
echo "╠══════════════════════════════════════════╣"
echo "║                                          ║"
echo "║  Now run the setup wizard:               ║"
echo "║                                          ║"
echo "║  cd ~/joob && joob-exit setup             ║"
echo "║                                          ║"
echo "║  You need a Google OAuth Client ID.      ║"
echo "║  Create one at:                          ║"
echo "║  console.cloud.google.com/apis/credentials║"
echo "║  Type: Desktop app                       ║"
echo "║                                          ║"
echo "║  After setup, start the service:         ║"
echo "║  systemctl start joob                    ║"
echo "║                                          ║"
echo "╚══════════════════════════════════════════╝"
echo ""
