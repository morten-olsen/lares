# Development

How to build, run, and test Lares locally.

## Build

```sh
cargo build --workspace
```

The binaries end up at `target/debug/laresd` (daemon) and `target/debug/lares` (CLI).

## Configuration

The daemon reads its config from `lares.toml`. See the [Configuration Guide](./config.md) for a full list of options.

For development, override the config path with the `LARES_CONFIG` env var:

```sh
cat > /tmp/lares-dev.toml << 'EOF'
[api]
key = "sk-or-v1-..."

[paths]
config_repo = "/tmp/lares-dev"
EOF

export LARES_CONFIG=/tmp/lares-dev.toml
```

Alternatively, set the API key via env var (no config file needed for the daemon):

```sh
export OPENROUTER_API_KEY="sk-or-v1-..."
```

## Running the daemon

### As your user (simplest, sufficient for development)

Terminal 1:

```sh
RUST_LOG=debug cargo run --bin laresd
```

You should see:

```
INFO laresd: laresd listening on /tmp/lares-501/lares.sock
```

The socket path includes your UID (501 is typical on macOS).

### As root on macOS

To test the root-daemon behavior:

```sh
sudo OPENROUTER_API_KEY="sk-or-v1-..." cargo run --bin laresd
```

Note: `sudo` does not inherit your shell environment by default. You must pass the env var explicitly, or use `sudo -E` (which forwards your entire environment — only use this in development).

The socket will be at `/tmp/lares-0/lares.sock` (UID 0). The CLI needs to know this:

```sh
cargo run --bin lares -- --socket /tmp/lares-0/lares.sock "your prompt here"
```

## Using the CLI

### Send a prompt

Terminal 2 (while the daemon is running):

```sh
cargo run --bin lares -- "what packages do I have installed"
```

If the daemon is running as root with a non-default socket:

```sh
cargo run --bin lares -- --socket /tmp/lares-0/lares.sock "your prompt here"
```

### Initialize a config repo

`lares init` scaffolds a config repo at the platform default path (`/Library/Lares` on macOS, `/etc/lares` on Linux). For development, use `LARES_CONFIG` to point at a test directory:

```sh
# Create a dev config pointing to a temp dir
cat > /tmp/lares-dev.toml << 'EOF'
[api]
key = "sk-or-v1-..."

[paths]
config_repo = "/tmp/lares-dev"
EOF

mkdir -p /tmp/lares-dev
LARES_CONFIG=/tmp/lares-dev.toml cargo run --bin lares -- init
```

Init prompts for an API key interactively (or picks it up from `OPENROUTER_API_KEY`). The scaffolded repo will contain:

```
flake.nix
system/default.nix
users/<username>/default.nix
users/<username>/packages.nix
users/<username>/shell.nix
lares/tasks/<username>/
lares/automations/shared/default.nix
lares/automations/<username>/default.nix
lares.toml                              (gitignored)
```

To test adopt mode against an existing repo:

```sh
# Create a fake existing nix config
mkdir -p /tmp/lares-adopt-test
echo '{ outputs = { self, ... }: { darwinConfigurations.test = {}; }; }' > /tmp/lares-adopt-test/flake.nix
cd /tmp/lares-adopt-test && git init && git add -A && git commit -m "init" && cd -

# Adopt it (only adds lares/ directories)
cargo run --bin lares -- init --repo /tmp/lares-adopt-test
```

### Check help

```sh
cargo run --bin lares -- --help
cargo run --bin lares -- init --help
```

## Debugging

### Verbose daemon logging

```sh
RUST_LOG=debug cargo run --bin laresd
```

This shows the full request/response cycle with the LLM, including the system prompt, tool calls, and agent reasoning.

### Inspect the generated system prompt

The system prompt is built in `crates/lares-core/src/context.rs`. With `RUST_LOG=debug`, the daemon logs the full prompt it sends to the LLM. Look for the tier detection, repo map, and "where to put changes" table to verify they match the detected platform.

### Common issues

**"connecting to /tmp/lares-501/lares.sock — is laresd running?"**
The daemon isn't running, or the CLI is looking at a different socket. If the daemon is running as root, its socket is at `/tmp/lares-0/lares.sock` — use `--socket` to point the CLI there.

**"API key not set"**
The env var isn't set in the daemon's environment. Remember that `sudo` doesn't forward env vars by default.

**Permission denied on socket**
The socket is created with the daemon's UID. If the daemon runs as root and the CLI runs as your user, the socket permissions may prevent access. For development, running both as your user avoids this.

## Testing changes

### After modifying templates or init logic

```sh
# Clean test with config override
rm -rf /tmp/lares-scaffold-test
mkdir /tmp/lares-scaffold-test

cat > /tmp/lares-test.toml << 'EOF'
[api]
key = "test"
[paths]
config_repo = "/tmp/lares-scaffold-test"
EOF

echo "test-key" | LARES_CONFIG=/tmp/lares-test.toml cargo run --bin lares -- init

# Inspect the result
find /tmp/lares-scaffold-test -not -path '*/.git/*' -not -name '.git' | sort
cat /tmp/lares-scaffold-test/flake.nix
```

### After modifying the system prompt

```sh
RUST_LOG=debug cargo run --bin laresd 2>&1 | head -200
# In another terminal:
cargo run --bin lares -- "test"
# Check the daemon's stderr for the full system prompt
```

### Full build check

```sh
cargo build --workspace
```

No warnings should appear. The workspace includes lares-protocol, lares-core, laresd, and lares-cli.
