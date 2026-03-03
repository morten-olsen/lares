#!/usr/bin/env bash
#
# Lares Daemon Installation Script
#
# This script installs the lares daemon as a system service.
# It must be run as root (sudo ./install.sh)
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
   echo "Usage: sudo ./install.sh"
   exit 1
fi

echo -e "${GREEN}Lares Daemon Installation${NC}"
echo "Platform: $PLATFORM"
echo ""

# Find binaries
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ -f "$PROJECT_ROOT/target/release/laresd" ]]; then
    LARESD_BIN="$PROJECT_ROOT/target/release/laresd"
elif [[ -f "$PROJECT_ROOT/target/debug/laresd" ]]; then
    LARESD_BIN="$PROJECT_ROOT/target/debug/laresd"
    echo -e "${YELLOW}Warning: Using debug build. Run 'cargo build --release' for production.${NC}"
else
    echo -e "${RED}Error: laresd binary not found${NC}"
    echo "Please build the project first: cargo build --release"
    exit 1
fi

if [[ -f "$PROJECT_ROOT/target/release/lares" ]]; then
    LARES_BIN="$PROJECT_ROOT/target/release/lares"
elif [[ -f "$PROJECT_ROOT/target/debug/lares" ]]; then
    LARES_BIN="$PROJECT_ROOT/target/debug/lares"
else
    echo -e "${RED}Error: lares binary not found${NC}"
    echo "Please build the project first: cargo build --release"
    exit 1
fi

# Install binaries
echo "Installing binaries..."
install -m 755 "$LARESD_BIN" /usr/local/bin/laresd
install -m 755 "$LARES_BIN" /usr/local/bin/lares
echo -e "${GREEN}✓${NC} Installed laresd and lares to /usr/local/bin/"

# Platform-specific installation
if [[ "$PLATFORM" == "macos" ]]; then
    echo ""
    echo "Installing macOS LaunchDaemon..."
    
    # Create log directory
    mkdir -p /var/log/lares
    chmod 755 /var/log/lares
    
    # Install plist
    install -m 644 "$SCRIPT_DIR/ai.lares.daemon.plist" /Library/LaunchDaemons/ai.lares.daemon.plist
    echo -e "${GREEN}✓${NC} Installed LaunchDaemon plist"
    
    # Load the service
    launchctl unload /Library/LaunchDaemons/ai.lares.daemon.plist 2>/dev/null || true
    launchctl load -w /Library/LaunchDaemons/ai.lares.daemon.plist
    echo -e "${GREEN}✓${NC} Started lares daemon"
    
    echo ""
    echo -e "${GREEN}Installation complete!${NC}"
    echo ""
    echo "Service management commands:"
    echo "  Start:   sudo launchctl load -w /Library/LaunchDaemons/ai.lares.daemon.plist"
    echo "  Stop:    sudo launchctl unload /Library/LaunchDaemons/ai.lares.daemon.plist"
    echo "  Restart: sudo launchctl kickstart -k system/ai.lares.daemon"
    echo "  Logs:    tail -f /var/log/lares/laresd.log"
    
elif [[ "$PLATFORM" == "linux" ]]; then
    echo ""
    echo "Installing systemd service..."
    
    # Install service file
    install -m 644 "$SCRIPT_DIR/lares.service" /etc/systemd/system/lares.service
    echo -e "${GREEN}✓${NC} Installed systemd service"
    
    # Reload systemd
    systemctl daemon-reload
    
    # Enable and start service
    systemctl enable lares.service
    systemctl start lares.service
    echo -e "${GREEN}✓${NC} Started lares daemon"
    
    echo ""
    echo -e "${GREEN}Installation complete!${NC}"
    echo ""
    echo "Service management commands:"
    echo "  Start:   sudo systemctl start lares"
    echo "  Stop:    sudo systemctl stop lares"
    echo "  Restart: sudo systemctl restart lares"
    echo "  Status:  sudo systemctl status lares"
    echo "  Logs:    sudo journalctl -u lares -f"
    
else
    echo -e "${RED}Error: Unsupported platform${NC}"
    exit 1
fi

echo ""
echo "Socket path:"
if [[ "$PLATFORM" == "macos" ]]; then
    echo "  /var/run/lares/lares.sock"
else
    echo "  /run/lares/lares.sock"
fi

echo ""
echo -e "${YELLOW}IMPORTANT: Configuration Setup${NC}"
echo ""

# Check if user has config at ~/.config/lares.toml
USER_HOME=$(eval echo ~${SUDO_USER:-$USER})
USER_CONFIG="$USER_HOME/.config/lares.toml"

if [[ -f "$USER_CONFIG" ]]; then
    echo "Found existing config at: $USER_CONFIG"
    
    if [[ "$PLATFORM" == "macos" ]]; then
        SYSTEM_CONFIG="/Library/Lares/lares.toml"
    else
        SYSTEM_CONFIG="/etc/lares/lares.toml"
    fi
    
    # Ask if they want to copy it
    echo ""
    read -p "Copy user config to system location for service? (recommended) [Y/n] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
        # Ensure parent directory exists
        mkdir -p "$(dirname "$SYSTEM_CONFIG")"
        
        # Copy config
        cp "$USER_CONFIG" "$SYSTEM_CONFIG"
        chmod 644 "$SYSTEM_CONFIG"
        echo -e "${GREEN}✓${NC} Copied config to $SYSTEM_CONFIG"
        
        # Restart service to pick up new config
        echo "Restarting service to load configuration..."
        if [[ "$PLATFORM" == "macos" ]]; then
            launchctl kickstart -k system/ai.lares.daemon
        else
            systemctl restart lares.service
        fi
        echo -e "${GREEN}✓${NC} Service restarted with new config"
    else
        echo ""
        echo -e "${YELLOW}Warning: Service won't have config!${NC}"
        echo "You'll need to create config at: $SYSTEM_CONFIG"
        echo "Or set LARES_CONFIG environment variable in the service file."
    fi
else
    echo -e "${YELLOW}No config found at: $USER_CONFIG${NC}"
    echo ""
    echo "Please create a config file with your OpenRouter API key:"
    if [[ "$PLATFORM" == "macos" ]]; then
        echo "  Location: /Library/Lares/lares.toml"
    else
        echo "  Location: /etc/lares/lares.toml"
    fi
    echo ""
    echo "Example config:"
    echo '  openrouter_api_key = "sk-or-v1-..."'
    echo '  config_repo = "/Library/Lares"  # macOS'
    echo '  # config_repo = "/etc/lares"    # Linux'
fi

echo ""
echo "Next steps:"
echo "  1. Verify service is running: ./status.sh"
if [[ ! -f "$USER_CONFIG" ]] || [[ $REPLY =~ ^[Nn]$ ]]; then
    echo "  2. Create/copy config to system location (see above)"
    echo "  3. Restart service: sudo ./restart.sh"
    echo "  4. Initialize your Nix configuration: sudo lares init"
    echo "  5. Test the CLI: lares \"what version of $(uname -s) am I running\""
else
    echo "  2. Initialize your Nix configuration: sudo lares init"
    echo "  3. Test the CLI: lares \"what version of $(uname -s) am I running\""
fi
echo ""
