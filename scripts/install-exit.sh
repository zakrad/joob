#!/bin/bash
set -e

echo "╔══════════════════════════════════════════╗"
echo "║     JOOB EXIT NODE — AUTO INSTALLER      ║"
echo "╚══════════════════════════════════════════╝"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Run as root: sudo bash install-exit.sh"
    exit 1
fi

# Ask for Google OAuth credentials upfront
# Read from /dev/tty so it works even when piped from curl
echo "You need a Google Cloud OAuth app (free)."
echo "Guide: https://github.com/zakrad/joob/blob/master/docs/SETUP.md"
echo ""
read -rp "Google Client ID: " CLIENT_ID < /dev/tty
if [ -z "$CLIENT_ID" ]; then
    echo "ERROR: Client ID is required."
    exit 1
fi
read -rp "Google Client Secret: " CLIENT_SECRET < /dev/tty
if [ -z "$CLIENT_SECRET" ]; then
    echo "ERROR: Client Secret is required."
    exit 1
fi
echo ""

# Install git if not present
if ! command -v git &> /dev/null; then
    echo "[1/6] Installing git..."
    apt-get update -qq && apt-get install -y -qq git > /dev/null
else
    echo "[1/6] git already installed."
fi

# Install Rust if not present
if ! command -v cargo &> /dev/null; then
    echo "[2/6] Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "[2/6] Rust already installed."
fi

# Clone and build
if [ ! -d "$HOME/joob" ]; then
    echo "[3/6] Cloning Joob..."
    git clone https://github.com/zakrad/joob.git "$HOME/joob"
else
    echo "[3/6] Joob already cloned, pulling latest..."
    cd "$HOME/joob" && git pull
fi

echo "[4/6] Building (this takes ~2 minutes on first run)..."
cd "$HOME/joob"
cargo build --release --quiet 2>/dev/null
cp target/release/joob-exit "$HOME/joob-exit"
chmod +x "$HOME/joob-exit"

echo "[5/6] Installing systemd service..."
cat > /etc/systemd/system/joob.service << EOF
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

# Run setup with the credentials
echo "[6/6] Running setup wizard..."
echo ""
"$HOME/joob-exit" setup --client-id "$CLIENT_ID" --client-secret "$CLIENT_SECRET"

# Start the service
echo ""
echo "Starting joob exit node..."
systemctl start joob

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║          SETUP COMPLETE ✓                ║"
echo "╠══════════════════════════════════════════╣"
echo "║                                          ║"
echo "║  Joob is running! Copy the joob://...    ║"
echo "║  profile string above to your client.    ║"
echo "║                                          ║"
echo "║  Useful commands:                        ║"
echo "║    systemctl status joob                 ║"
echo "║    journalctl -u joob -f                 ║"
echo "║    systemctl restart joob                ║"
echo "║                                          ║"
echo "╚══════════════════════════════════════════╝"
