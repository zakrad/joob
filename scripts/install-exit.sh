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

# Install Rust if not present
if ! command -v cargo &> /dev/null; then
    echo "[1/5] Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "[1/5] Rust already installed."
fi

# Clone and build
if [ ! -d "$HOME/joob" ]; then
    echo "[2/5] Cloning Joob..."
    git clone https://github.com/zakrad/joob.git "$HOME/joob"
else
    echo "[2/5] Joob already cloned, pulling latest..."
    cd "$HOME/joob" && git pull
fi

echo "[3/5] Building (this takes ~60 seconds)..."
cd "$HOME/joob"
cargo build --release --quiet 2>/dev/null
cp target/release/joob-exit "$HOME/joob-exit"
chmod +x "$HOME/joob-exit"

echo "[4/5] Binary ready at ~/joob-exit"

# Install systemd service
echo "[5/5] Installing systemd service..."
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

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║          INSTALL COMPLETE ✓              ║"
echo "╠══════════════════════════════════════════╣"
echo "║                                          ║"
echo "║  Next: run the setup wizard:             ║"
echo "║                                          ║"
echo "║  ~/joob-exit setup \\                     ║"
echo "║    --client-id \"YOUR_ID\" \\               ║"
echo "║    --client-secret \"YOUR_SECRET\"          ║"
echo "║                                          ║"
echo "║  Then start:                             ║"
echo "║    systemctl start joob                  ║"
echo "║                                          ║"
echo "╚══════════════════════════════════════════╝"
