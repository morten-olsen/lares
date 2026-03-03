# Service Troubleshooting Guide

Common issues when running lares as a system service and how to fix them.

## Issue: "API key not set"

**Error in logs:**
```
ERROR laresd: agent task failed: API key not set. Set OPENROUTER_API_KEY or add key to lares.toml
```

**Cause:** The service can't find the config file with your API key.

**Solution:**

The service runs as root and reads config from the **system location**, not from `~/.config/lares.toml`.

1. Copy your config to the system location:

   ```sh
   # macOS
   sudo cp ~/.config/lares.toml /Library/Lares/lares.toml
   
   # Linux
   sudo cp ~/.config/lares.toml /etc/lares/lares.toml
   ```

2. Restart the service:

   ```sh
   cd service
   sudo ./restart.sh
   ```

3. Verify it's working:

   ```sh
   ./status.sh
   lares "test prompt"
   ```

## Issue: "Nix is required but was not detected"

**Error in logs:**
```
ERROR laresd: agent task failed: Nix is required but was not detected.
```

**Cause:** The service doesn't have Nix binaries in its PATH.

**Solution:**

Update the service configuration to include Nix in the PATH:

1. Verify the updated plist includes Nix paths:

   ```sh
   grep -A5 PATH service/ai.lares.daemon.plist
   ```
   
   Should show:
   ```xml
   <key>PATH</key>
   <string>/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin</string>
   ```

2. Update the installed plist:

   ```sh
   cd service
   sudo cp ai.lares.daemon.plist /Library/LaunchDaemons/
   ```

3. Reload the service:

   ```sh
   sudo launchctl unload /Library/LaunchDaemons/ai.lares.daemon.plist
   sudo launchctl load -w /Library/LaunchDaemons/ai.lares.daemon.plist
   ```

4. Verify:

   ```sh
   ./status.sh
   lares "which nix binary am I using"
   ```

**Alternative:** Set PATH in the plist's EnvironmentVariables section manually if your Nix is in a different location.

## Issue: "Broken pipe" errors

**Error in logs:**
```
ERROR laresd: agent task failed: Broken pipe (os error 32)
```

**Cause:** Fixed in recent versions - was sending duplicate completion events.

**Solution:** Rebuild and reinstall:

```sh
cargo build --release
cd service
sudo ./restart.sh
```

## Issue: "Running Homebrew as root"

**Error in logs:**
```
Warning: Running Homebrew as root is extremely dangerous and no longer supported.
```

**Cause:** Fixed in recent versions - commands now run with dropped privileges.

**Solution:** Rebuild and reinstall:

```sh
cargo build --release
cd service
sudo ./restart.sh
```

## Issue: Service won't start

**Symptoms:** `./status.sh` shows service not running.

**Diagnosis steps:**

1. Check logs for errors:

   ```sh
   # macOS
   tail -50 /var/log/lares/laresd.log
   
   # Linux
   sudo journalctl -u lares -n 50
   ```

2. Try running manually to see full errors:

   ```sh
   sudo ./stop.sh
   sudo laresd
   # Watch for errors, then Ctrl+C
   sudo ./start.sh
   ```

3. Common issues:
   - Missing config file → See "API key not set" above
   - Invalid config syntax → Check TOML syntax
   - Socket permission errors → Check `/var/run/lares/` permissions
   - Binary not found → Reinstall with `./install.sh`

## Issue: Socket permission denied

**Error:** "connecting to socket — permission denied"

**Solution:**

1. Check socket permissions:

   ```sh
   ls -la /var/run/lares/lares.sock
   ```
   
   Should be `srw-rw-rw-` (0666)

2. If wrong, restart service:

   ```sh
   sudo ./restart.sh
   ```

3. If still wrong, check the daemon code creates socket with correct permissions.

## Config Priority Reference

When running as a service, config is loaded in this order:

1. `$LARES_CONFIG` environment variable (if set in plist)
2. **System location** (recommended):
   - macOS: `/Library/Lares/lares.toml`
   - Linux: `/etc/lares/lares.toml`
3. User location (fallback, not used by service):
   - `~/.config/lares.toml`

**For service use, always put config at the system location.**

## Getting Help

If none of these solutions work:

1. Run status script with sudo for full diagnostics:

   ```sh
   sudo ./status.sh
   ```

2. Enable debug logging:

   ```sh
   # Edit the plist
   sudo vim /Library/LaunchDaemons/ai.lares.daemon.plist
   # Change RUST_LOG from "info" to "debug"
   
   sudo ./restart.sh
   tail -f /var/log/lares/laresd.log
   ```

3. Check GitHub issues or create a new one with:
   - Output from `./status.sh`
   - Last 50 lines of logs
   - Your platform (macOS/Linux)
   - How you installed (service vs manual)
