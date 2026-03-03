#!/usr/bin/env bash
#
# Restart the lares daemon service
#

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

# Detect platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    PLATFORM="macos"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    PLATFORM="linux"
else
    echo -e "${RED}Error: Unsupported platform${NC}"
    exit 1
fi

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}Error: This script must be run as root${NC}"
   echo "Usage: sudo ./restart.sh"
   exit 1
fi

echo "Restarting lares daemon..."

if [[ "$PLATFORM" == "macos" ]]; then
    if [[ ! -f /Library/LaunchDaemons/ai.lares.daemon.plist ]]; then
        echo -e "${RED}Error: Service not installed${NC}"
        echo "Run: sudo ./install.sh"
        exit 1
    fi
    
    launchctl kickstart -k system/ai.lares.daemon
    echo -e "${GREEN}✓ Restarted lares daemon${NC}"
    echo ""
    echo "View logs: tail -f /var/log/lares/laresd.log"
    
elif [[ "$PLATFORM" == "linux" ]]; then
    if [[ ! -f /etc/systemd/system/lares.service ]]; then
        echo -e "${RED}Error: Service not installed${NC}"
        echo "Run: sudo ./install.sh"
        exit 1
    fi
    
    systemctl restart lares.service
    echo -e "${GREEN}✓ Restarted lares daemon${NC}"
    echo ""
    echo "View logs: sudo journalctl -u lares -f"
    echo "Status: sudo systemctl status lares"
fi
