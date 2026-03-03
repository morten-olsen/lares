#!/usr/bin/env bash
#
# Lares Daemon Uninstallation Script
#
# This script removes the lares daemon service.
# It must be run as root (sudo ./uninstall.sh)
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Detect platform
detect_platform() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "macos"
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        echo "linux"
    else
        echo "unknown"
    fi
}

PLATFORM=$(detect_platform)

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}Error: This script must be run as root${NC}"
   echo "Usage: sudo ./uninstall.sh"
   exit 1
fi

echo -e "${YELLOW}Lares Daemon Uninstallation${NC}"
echo "Platform: $PLATFORM"
echo ""

# Ask for confirmation
read -p "Are you sure you want to uninstall lares daemon? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Uninstallation cancelled."
    exit 0
fi

# Platform-specific uninstallation
if [[ "$PLATFORM" == "macos" ]]; then
    echo "Uninstalling macOS LaunchDaemon..."
    
    # Stop and unload the service
    if [[ -f /Library/LaunchDaemons/ai.lares.daemon.plist ]]; then
        launchctl unload /Library/LaunchDaemons/ai.lares.daemon.plist 2>/dev/null || true
        rm -f /Library/LaunchDaemons/ai.lares.daemon.plist
        echo -e "${GREEN}✓${NC} Removed LaunchDaemon"
    else
        echo -e "${YELLOW}LaunchDaemon not found, skipping${NC}"
    fi
    
    # Remove log directory (ask first)
    if [[ -d /var/log/lares ]]; then
        read -p "Remove log directory /var/log/lares? (y/N) " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            rm -rf /var/log/lares
            echo -e "${GREEN}✓${NC} Removed log directory"
        fi
    fi
    
elif [[ "$PLATFORM" == "linux" ]]; then
    echo "Uninstalling systemd service..."
    
    # Stop and disable service
    if systemctl is-active --quiet lares.service; then
        systemctl stop lares.service
        echo -e "${GREEN}✓${NC} Stopped service"
    fi
    
    if systemctl is-enabled --quiet lares.service 2>/dev/null; then
        systemctl disable lares.service
        echo -e "${GREEN}✓${NC} Disabled service"
    fi
    
    # Remove service file
    if [[ -f /etc/systemd/system/lares.service ]]; then
        rm -f /etc/systemd/system/lares.service
        systemctl daemon-reload
        echo -e "${GREEN}✓${NC} Removed systemd service"
    else
        echo -e "${YELLOW}Service file not found, skipping${NC}"
    fi
    
else
    echo -e "${RED}Error: Unsupported platform${NC}"
    exit 1
fi

# Remove binaries
if [[ -f /usr/local/bin/laresd ]]; then
    rm -f /usr/local/bin/laresd
    echo -e "${GREEN}✓${NC} Removed /usr/local/bin/laresd"
fi

if [[ -f /usr/local/bin/lares ]]; then
    rm -f /usr/local/bin/lares
    echo -e "${GREEN}✓${NC} Removed /usr/local/bin/lares"
fi

# Remove socket directory
if [[ "$PLATFORM" == "macos" ]]; then
    SOCKET_DIR="/var/run/lares"
else
    SOCKET_DIR="/run/lares"
fi

if [[ -d "$SOCKET_DIR" ]]; then
    rm -rf "$SOCKET_DIR"
    echo -e "${GREEN}✓${NC} Removed socket directory"
fi

echo ""
echo -e "${GREEN}Uninstallation complete!${NC}"
echo ""
echo "Note: Configuration files were NOT removed."
echo "To completely remove lares, also delete:"
if [[ "$PLATFORM" == "macos" ]]; then
    echo "  - /Library/Lares/ (config repository)"
else
    echo "  - /etc/lares/ (config repository)"
fi
echo "  - ~/.config/lares.toml (user config, if present)"
echo ""
