#!/usr/bin/env bash
#
# Stop the lares daemon service
#

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
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
   echo "Usage: sudo ./stop.sh"
   exit 1
fi

echo "Stopping lares daemon..."

if [[ "$PLATFORM" == "macos" ]]; then
    if ! launchctl list | grep -q "ai.lares.daemon"; then
        echo -e "${YELLOW}Service is not running${NC}"
        exit 0
    fi
    
    launchctl unload /Library/LaunchDaemons/ai.lares.daemon.plist
    echo -e "${GREEN}✓ Stopped lares daemon${NC}"
    echo ""
    echo "Note: Service will NOT start on next boot unless you run ./start.sh"
    
elif [[ "$PLATFORM" == "linux" ]]; then
    if ! systemctl is-active --quiet lares.service; then
        echo -e "${YELLOW}Service is not running${NC}"
        exit 0
    fi
    
    systemctl stop lares.service
    echo -e "${GREEN}✓ Stopped lares daemon${NC}"
    echo ""
    echo "Note: Service is still enabled and will start on next boot"
    echo "To disable auto-start: sudo systemctl disable lares"
fi
