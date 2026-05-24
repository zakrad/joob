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

# Check if joob-exit binary exists
if [ ! -f "$HOME/joob-exit" ]; then
    echo "ERROR: ~/joob-exit binary not found."
    echo ""
    echo "First, copy it from your local machine:"
    echo "  scp /path/to/joob-exit root@THIS_SERVER:~/joob-exit"
    echo ""
    exit 1
fi

chmod +x "$HOME/joob-exit"

echo "[1/2] Binary found at ~/joob-exit"

# Install systemd service
echo "[2/2] Installing systemd service..."
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
