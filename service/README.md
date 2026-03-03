# Lares Service Installation

This directory contains files for installing and managing lares as a system service.

## Quick Install

```sh
# From the project root:
cargo build --release
cd service
sudo ./install.sh
```

## Service Management Scripts

After installation, use these scripts to manage the service:

```sh
sudo ./start.sh      # Start the service
sudo ./stop.sh       # Stop the service
sudo ./restart.sh    # Restart the service
./status.sh          # Check status and view logs (sudo for full info)
sudo ./uninstall.sh  # Completely remove the service
```

## Files

**Management Scripts:**
- **`install.sh`** - Install and start the service
- **`start.sh`** - Start the service
- **`stop.sh`** - Stop the service
- **`restart.sh`** - Restart the service
- **`status.sh`** - Check status and view logs
- **`uninstall.sh`** - Uninstall the service

**Configuration Files:**
- **`ai.lares.daemon.plist`** - macOS LaunchDaemon configuration
- **`lares.service`** - Linux systemd service configuration

## Requirements

- **Root access** - Service installation requires sudo
- **Built binaries** - Run `cargo build --release` first
- **Platform**: macOS or Linux

## What It Does

The install script:
1. Copies binaries to `/usr/local/bin/`
2. Installs platform-specific service files
3. Starts the daemon automatically
4. Configures auto-start on boot

## Platform Support

### macOS
- Uses LaunchDaemon (runs as root on boot)
- Logs to `/var/log/lares/laresd.log`
- Socket at `/var/run/lares/lares.sock`

### Linux
- Uses systemd (runs as root on boot)
- Logs to journald
- Socket at `/run/lares/lares.sock`

## Post-Installation

After installation, initialize your configuration:

```sh
sudo lares init
```

Then test:

```sh
lares "what version of $(uname -s) am I running"
```

## Common Tasks

**Check if service is running:**
```sh
./status.sh
# or with full details:
sudo ./status.sh
```

**View live logs:**
```sh
# macOS
tail -f /var/log/lares/laresd.log

# Linux
sudo journalctl -u lares -f
```

**Restart after code changes:**
```sh
cargo build --release
sudo ./restart.sh
```

**Temporarily stop the service:**
```sh
sudo ./stop.sh
```

**Start it again:**
```sh
sudo ./start.sh
```

## Documentation

See [docs/installation.md](../docs/installation.md) for detailed installation guide, troubleshooting, and manual installation instructions.
