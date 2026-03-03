# Installation Guide

This guide covers installing lares as a system service on macOS and Linux.

## Quick Install

```sh
# Build the project
cargo build --release

# Install as system service (requires root)
cd service
sudo ./install.sh
```

That's it! The daemon is now running and will start automatically on boot.

## What Gets Installed

The installation script:

1. **Copies binaries** to `/usr/local/bin/`:
   - `laresd` - the daemon (runs as root)
   - `lares` - the CLI (runs as user)

2. **Installs system service**:
   - **macOS**: LaunchDaemon at `/Library/LaunchDaemons/ai.lares.daemon.plist`
   - **Linux**: systemd service at `/etc/systemd/system/lares.service`

3. **Creates directories**:
   - **macOS**: `/var/run/lares/` (socket) and `/var/log/lares/` (logs)
   - **Linux**: `/run/lares/` (socket, ephemeral)

4. **Starts the daemon** automatically

## Platform-Specific Details

### macOS (LaunchDaemon)

The daemon is installed as a LaunchDaemon, which means:
- ✅ Runs as root with full system privileges
- ✅ Starts automatically on boot
- ✅ Restarts automatically if it crashes
- ✅ Logs to `/var/log/lares/laresd.log`

**Service management:**
```sh
# Start
sudo launchctl load -w /Library/LaunchDaemons/ai.lares.daemon.plist

# Stop
sudo launchctl unload /Library/LaunchDaemons/ai.lares.daemon.plist

# Restart
sudo launchctl kickstart -k system/ai.lares.daemon

# View logs
tail -f /var/log/lares/laresd.log

# View logs with more detail
sudo RUST_LOG=debug launchctl kickstart -k system/ai.lares.daemon
tail -f /var/log/lares/laresd.log
```

**Plist location:** `/Library/LaunchDaemons/ai.lares.daemon.plist`

### Linux (systemd)

The daemon is installed as a systemd service, which means:
- ✅ Runs as root with full system privileges  
- ✅ Starts automatically on boot
- ✅ Restarts automatically if it crashes (5s delay)
- ✅ Logs to journald

**Service management:**
```sh
# Start
sudo systemctl start lares

# Stop
sudo systemctl stop lares

# Restart
sudo systemctl restart lares

# Status
sudo systemctl status lares

# View logs (live)
sudo journalctl -u lares -f

# View logs with more detail
sudo systemctl set-environment RUST_LOG=debug
sudo systemctl restart lares
sudo journalctl -u lares -f
```

**Service file location:** `/etc/systemd/system/lares.service`

## Socket Location

The daemon listens on a Unix socket:
- **macOS**: `/var/run/lares/lares.sock`
- **Linux**: `/run/lares/lares.sock`

The socket has `0666` permissions, allowing all users to connect.

## User Identity

When you run `lares "prompt"`, the CLI connects to the daemon's socket. The daemon uses **SO_PEERCRED** (Linux) or **getpeereid** (macOS) to identify your UID/GID from the socket connection.

This means:
- ✅ Daemon runs as root (for system access)
- ✅ Commands execute as **your user** (with dropped privileges)
- ✅ Task journals are owned by **your user**
- ✅ Nix config changes are scoped to **your user** directory

## Configuration

The daemon requires a configuration file with your OpenRouter API key at the **system location**:

- **macOS**: `/Library/Lares/lares.toml`
- **Linux**: `/etc/lares/lares.toml`

**Important**: The service runs as root and reads config from the system location, **not** from `~/.config/lares.toml`.

### Config Priority

When the daemon starts, it looks for config in this order:
1. `$LARES_CONFIG` environment variable (if set)
2. System location (`/Library/Lares/lares.toml` or `/etc/lares/lares.toml`)
3. User location (`~/.config/lares.toml`) - fallback only, not used when running as service

### Setting Up Config

If you have an existing config at `~/.config/lares.toml`, copy it to the system location:

```sh
# macOS
sudo cp ~/.config/lares.toml /Library/Lares/lares.toml

# Linux
sudo cp ~/.config/lares.toml /etc/lares/lares.toml
```

Or create a new one directly:

```sh
# macOS
sudo mkdir -p /Library/Lares
sudo tee /Library/Lares/lares.toml > /dev/null << EOF
openrouter_api_key = "sk-or-v1-your-key-here"
config_repo = "/Library/Lares"

[build]
test_command = "make dry-run"
apply_command = "make switch"
git_author_name = "lares"
git_author_email = "lares@localhost"
EOF

# Linux
sudo mkdir -p /etc/lares
sudo tee /etc/lares/lares.toml > /dev/null << EOF
openrouter_api_key = "sk-or-v1-your-key-here"
config_repo = "/etc/lares"

[build]
test_command = "make dry-run"
apply_command = "make switch"
git_author_name = "lares"
git_author_email = "lares@localhost"
EOF
```

After creating/updating the config, restart the service:

```sh
cd service
sudo ./restart.sh
```

### Initialize Nix Configuration

After the config is in place, initialize your Nix configuration:

```sh
# Scaffold a new config repo
sudo lares init

# Or adopt an existing config
sudo lares init --repo /path/to/existing-nix-config

# Or clone from remote
sudo lares init --clone git@github.com:user/nix-config.git
```

## Testing the Installation

```sh
# Test basic connectivity
lares "what version of $(uname -s) am I running"

# Test Nix integration
lares "list my nix configuration files"

# Test package management
lares "install ripgrep via nix"
```

## Uninstalling

```sh
cd service
sudo ./uninstall.sh
```

This will:
- Stop and remove the system service
- Remove binaries from `/usr/local/bin/`
- Remove the socket directory
- **NOT** remove configuration files (you can delete those manually)

## Troubleshooting

### "connecting to socket — is laresd running?"

Check if the daemon is running:

```sh
# macOS
sudo launchctl list | grep lares
tail /var/log/lares/laresd.log

# Linux
sudo systemctl status lares
sudo journalctl -u lares -n 50
```

### "permission denied" on socket

Check socket permissions:

```sh
# macOS
ls -la /var/run/lares/lares.sock

# Linux
ls -la /run/lares/lares.sock
```

Should be `0666` (writable by all users).

### Service won't start

Check the logs for errors:

```sh
# macOS
tail -100 /var/log/lares/laresd.log

# Linux
sudo journalctl -u lares -n 100
```

Common issues:
- Missing dependencies (Nix not installed)
- Invalid config at `~/.config/lares.toml`
- API key not set

### Enable debug logging

**macOS:**
```sh
# Edit the plist
sudo vim /Library/LaunchDaemons/ai.lares.daemon.plist
# Change RUST_LOG value from "info" to "debug"
sudo launchctl unload /Library/LaunchDaemons/ai.lares.daemon.plist
sudo launchctl load -w /Library/LaunchDaemons/ai.lares.daemon.plist
tail -f /var/log/lares/laresd.log
```

**Linux:**
```sh
sudo systemctl edit lares
# Add:
# [Service]
# Environment="RUST_LOG=debug"
sudo systemctl restart lares
sudo journalctl -u lares -f
```

## Manual Installation

If you prefer not to use the install script:

### macOS

```sh
# Build
cargo build --release

# Copy binaries
sudo install -m 755 target/release/laresd /usr/local/bin/
sudo install -m 755 target/release/lares /usr/local/bin/

# Create log directory
sudo mkdir -p /var/log/lares
sudo chmod 755 /var/log/lares

# Install plist
sudo cp service/ai.lares.daemon.plist /Library/LaunchDaemons/
sudo chmod 644 /Library/LaunchDaemons/ai.lares.daemon.plist

# Load service
sudo launchctl load -w /Library/LaunchDaemons/ai.lares.daemon.plist
```

### Linux

```sh
# Build
cargo build --release

# Copy binaries
sudo install -m 755 target/release/laresd /usr/local/bin/
sudo install -m 755 target/release/lares /usr/local/bin/

# Install service
sudo cp service/lares.service /etc/systemd/system/
sudo chmod 644 /etc/systemd/system/lares.service

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable lares
sudo systemctl start lares
```

## Production Recommendations

1. **Set proper log rotation** (especially on macOS where logs go to files)
   
   Create `/etc/newsyslog.d/lares.conf`:
   ```
   /var/log/lares/laresd.log    644  7     *    @T00  J
   ```

2. **Configure resource limits** if needed (see service files)

3. **Set up monitoring/alerting** for daemon health

4. **Use release builds** in production (not debug builds)

5. **Keep binaries updated** - rebuild and reinstall after updates

6. **Backup your config repo** regularly (it contains your entire system config)
