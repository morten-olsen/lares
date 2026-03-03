#!/usr/bin/env bash
#
# Check lares daemon service status and show logs
#

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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

echo -e "${BLUE}=== Lares Daemon Status ===${NC}"
echo ""

if [[ "$PLATFORM" == "macos" ]]; then
    # Check if service is installed
    if [[ ! -f /Library/LaunchDaemons/ai.lares.daemon.plist ]]; then
        echo -e "${RED}✗ Service not installed${NC}"
        echo "Run: sudo ./install.sh"
        exit 1
    fi
    
    # Check if service is running
    if launchctl list | grep -q "ai.lares.daemon"; then
        echo -e "${GREEN}✓ Service is running${NC}"
        
        # Get PID
        PID=$(launchctl list | grep ai.lares.daemon | awk '{print $1}')
        if [[ "$PID" != "-" ]]; then
            echo "  PID: $PID"
        fi
    else
        echo -e "${RED}✗ Service is not running${NC}"
    fi
    
    # Check socket
    if [[ -e /var/run/lares/lares.sock ]]; then
        echo -e "${GREEN}✓ Socket exists${NC}"
        echo "  Path: /var/run/lares/lares.sock"
        ls -l /var/run/lares/lares.sock | awk '{print "  Permissions: " $1 " Owner: " $3 ":" $4}'
    else
        echo -e "${YELLOW}⚠ Socket not found${NC}"
    fi
    
    # Check binaries
    echo ""
    echo "Binaries:"
    if [[ -f /usr/local/bin/laresd ]]; then
        echo -e "  ${GREEN}✓${NC} /usr/local/bin/laresd"
    else
        echo -e "  ${RED}✗${NC} /usr/local/bin/laresd (missing)"
    fi
    
    if [[ -f /usr/local/bin/lares ]]; then
        echo -e "  ${GREEN}✓${NC} /usr/local/bin/lares"
    else
        echo -e "  ${RED}✗${NC} /usr/local/bin/lares (missing)"
    fi
    
    # Show recent logs
    echo ""
    echo -e "${BLUE}=== Recent Logs (last 20 lines) ===${NC}"
    if [[ -f /var/log/lares/laresd.log ]]; then
        tail -20 /var/log/lares/laresd.log
        echo ""
        echo "View live logs: tail -f /var/log/lares/laresd.log"
    else
        echo -e "${YELLOW}No log file found${NC}"
    fi
    
elif [[ "$PLATFORM" == "linux" ]]; then
    # Check if service is installed
    if [[ ! -f /etc/systemd/system/lares.service ]]; then
        echo -e "${RED}✗ Service not installed${NC}"
        echo "Run: sudo ./install.sh"
        exit 1
    fi
    
    # Check if running as root (needed for systemctl status)
    if [[ $EUID -ne 0 ]]; then
        echo -e "${YELLOW}Note: Run with sudo for full status information${NC}"
        echo ""
    fi
    
    # Show systemd status
    systemctl status lares.service --no-pager || true
    
    echo ""
    
    # Check socket
    if [[ -e /run/lares/lares.sock ]]; then
        echo -e "${GREEN}✓ Socket exists${NC}"
        echo "  Path: /run/lares/lares.sock"
        ls -l /run/lares/lares.sock 2>/dev/null | awk '{print "  Permissions: " $1 " Owner: " $3 ":" $4}' || echo "  (run with sudo to see permissions)"
    else
        echo -e "${YELLOW}⚠ Socket not found${NC}"
    fi
    
    # Check binaries
    echo ""
    echo "Binaries:"
    if [[ -f /usr/local/bin/laresd ]]; then
        echo -e "  ${GREEN}✓${NC} /usr/local/bin/laresd"
    else
        echo -e "  ${RED}✗${NC} /usr/local/bin/laresd (missing)"
    fi
    
    if [[ -f /usr/local/bin/lares ]]; then
        echo -e "  ${GREEN}✓${NC} /usr/local/bin/lares"
    else
        echo -e "  ${RED}✗${NC} /usr/local/bin/lares (missing)"
    fi
    
    # Show recent logs
    echo ""
    echo -e "${BLUE}=== Recent Logs (last 20 lines) ===${NC}"
    if [[ $EUID -eq 0 ]]; then
        journalctl -u lares.service -n 20 --no-pager
        echo ""
        echo "View live logs: sudo journalctl -u lares -f"
    else
        echo -e "${YELLOW}Run with sudo to view logs: sudo ./status.sh${NC}"
    fi
fi

echo ""
