#!/usr/bin/env bash
#
# Start the lares daemon service
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
   echo "Usage: sudo ./start.sh"
   exit 1
fi

echo "Starting lares daemon..."

if [[ "$PLATFORM" == "macos" ]]; then
    if launchctl list | grep -q "ai.lares.daemon"; then
        echo -e "${GREEN}Service is already running${NC}"
        exit 0
    fi
    
    if [[ ! -f /Library/LaunchDaemons/ai.lares.daemon.plist ]]; then
        echo -e "${RED}Error: Service not installed${NC}"
        echo "Run: sudo ./install.sh"
        exit 1
    fi
    
    launchctl load -w /Library/LaunchDaemons/ai.lares.daemon.plist
    echo -e "${GREEN}✓ Started lares daemon${NC}"
    echo ""
    echo "View logs: tail -f /var/log/lares/laresd.log"
    
elif [[ "$PLATFORM" == "linux" ]]; then
    if systemctl is-active --quiet lares.service; then
        echo -e "${GREEN}Service is already running${NC}"
        exit 0
    fi
    
    if [[ ! -f /etc/systemd/system/lares.service ]]; then
        echo -e "${RED}Error: Service not installed${NC}"
        echo "Run: sudo ./install.sh"
        exit 1
    fi
    
    systemctl start lares.service
    echo -e "${GREEN}✓ Started lares daemon${NC}"
    echo ""
    echo "View logs: sudo journalctl -u lares -f"
    echo "Status: sudo systemctl status lares"
fi
