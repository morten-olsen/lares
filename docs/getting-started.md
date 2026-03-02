# Getting Started

## Prerequisites

- **Rust** (1.75+) — install via [rustup](https://rustup.rs/)
- **An OpenRouter API key** — sign up at [openrouter.ai](https://openrouter.ai/) and create an API key
- **Nix** — required. One of:
  - **NixOS** — full declarative system management
  - **macOS + nix-darwin + Home Manager** — declarative macOS config
  - **Linux + Nix + Home Manager** — declarative user environment on any distro

Install Nix: https://nixos.org/download
Install Home Manager: https://nix-community.github.io/home-manager/

## Build

From the repo root:

```sh
cargo build --release
```

The two binaries are:
- `target/release/laresd` — the daemon (runs as root)
- `target/release/lares` — the CLI (runs as user)

You can also run them directly during development with `cargo run --bin laresd` and `cargo run --bin lares`.

## Initializing

`lares init` scaffolds the config repo, creates the daemon configuration, and prompts for your API key. It requires root because the config repo and daemon config are system-owned.

```sh
sudo lares init
```

This detects your platform and creates:

**NixOS / Linux** at `/etc/lares/`:
```
/etc/lares/
  lares.toml                     # Daemon config (API key, model, etc)
  flake.nix                      # Tier-appropriate inputs + outputs
  system/default.nix             # System config (NixOS only, not Linux HM)
  users/<you>/default.nix        # Your Home Manager config
  users/<you>/packages.nix       # Your packages
  users/<you>/shell.nix          # Your shell config
  lares/tasks/<you>/             # Your task journal directory
  lares/automations/shared/      # System-wide automations
  lares/automations/<you>/       # Your automations
  .gitignore
```

**macOS** at `/Library/Lares/`:
```
/Library/Lares/
  lares.toml                     # Daemon config
  flake.nix                      # nix-darwin + Home Manager
  system/default.nix             # nix-darwin system config
  users/<you>/default.nix        # Your Home Manager config
  users/<you>/packages.nix       # Your packages
  users/<you>/shell.nix          # Your shell config
  lares/tasks/<you>/             # Your task journal directory
  lares/automations/shared/      # System-wide automations
  lares/automations/<you>/       # Your automations
  .gitignore
```

The generated flake.nix is tailored to your platform. Home Manager config is wired into the system module so a single rebuild applies everything.

### Adopting an existing config

If you already have a Nix config repo with a flake.nix:

```sh
sudo lares init --repo /path/to/existing-nix-config
```

This only adds the `lares/` overlay (task directory + automations). It never touches your existing flake.nix, system/, or user files. It also creates `/etc/lares/lares.toml` if it doesn't exist.

To enable automations, add this to your home manager imports:

```nix
++ (import ../../lares/automations/<username>/default.nix)
```

### Cloning a remote config

```sh
sudo lares init --clone git@github.com:user/nix-config.git
```

This clones the repo to the platform-appropriate location, then adopts it.

## Running

### 1. Start the daemon

The daemon runs as root (as a system service or directly):

```sh
sudo laresd
```

Or during development:

```sh
sudo cargo run --bin laresd
```

You should see:

```
INFO laresd: laresd listening on /run/lares/lares.sock
```

Set `RUST_LOG=debug` for verbose output.

### 2. Send a prompt

In another terminal (as your normal user):

```sh
lares "what version of macOS am I running"
```

Or during development:

```sh
cargo run --bin lares -- "what version of macOS am I running"
```

The daemon identifies you via the socket connection and scopes its actions to your config. The agent will use `run_command` with `sw_vers` (auto-approved as read-only) and return the answer.

### 3. Try a mutation

```sh
lares "list my nix configuration files"
```

Then something that requires approval:

```sh
lares "enable dark mode in my nix-darwin config"
```

The CLI will show the proposed file edit or command and ask you to approve:

```
Approval required: edit file
  Path: /Library/Lares/system/default.nix
  Description: Enable dark mode in nix-darwin configuration
  Content preview:
    { config, pkgs, ... }:
    ...

Approve? [y/N]
```

Type `y` to approve or `n` to reject. Rejected actions are reported back to the agent, which can adjust its approach.

## Task journals

Every interaction creates a task journal in `lares/tasks/<username>/`:

```
lares/tasks/alice/
  001-what-version-of-macos-am-i-running.md
  002-enable-dark-mode-in-my-nix-darwin-config.md
```

Each file contains YAML frontmatter (id, status, prompt) and a chronological journal of what the agent observed, did, and concluded. Journals are scoped to the user who initiated the task.

## CLI options

```
lares [OPTIONS] [PROMPT]...
lares init [--repo <PATH> | --clone <URL>]

Subcommands:
  init               Initialize or adopt a Nix config repo (requires sudo)

Arguments:
  [PROMPT]...        The prompt to send to the daemon

Options:
  --task <ID>        Resume an existing task by ID
  --socket <PATH>    Override the socket path
  -h, --help         Print help

Init options:
  --repo <PATH>      Adopt an existing local Nix config repo
  --clone <URL>      Clone a remote repo, then adopt it
```

## Troubleshooting

**"connecting to socket — is laresd running?"**
The daemon isn't running. Start it with `sudo laresd`.

**"API key not set"**
Run `sudo lares init` to set up the daemon config with your API key, or edit `lares.toml` in the config repo directly (`/etc/lares/lares.toml` on Linux, `/Library/Lares/lares.toml` on macOS).

**Agent does nothing / empty responses**
Check `RUST_LOG=debug sudo laresd` output for the full request/response cycle. Verify your OpenRouter API key has credits and the model you've configured is available.

**Permission denied on init**
`lares init` requires root. Use `sudo lares init`.

**Socket permission errors**
The daemon socket should be accessible to all local users. Check that the socket directory exists and has appropriate permissions.
