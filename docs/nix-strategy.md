# Nix Strategy

Lares uses Nix as the backbone for system configuration. Instead of running imperative commands that mutate system state, the agent edits declarative Nix configuration files and rebuilds. This gives us atomic changes, automatic rollback, full audit history, and reproducibility across machines.

## Why Nix

**Rollback is free.** Every `nixos-rebuild switch` or `home-manager switch` creates a new generation. Rolling back is `--rollback` or switching to any previous generation. The agent doesn't need to implement its own undo system — Nix IS the undo system.

**Audit is free.** The config repo is version-controlled. Every change is a git commit tied to a task. `git log` shows the full history of every change the agent ever made, with the reasoning (commit messages reference task journals).

**Dry runs are free.** `nix build` without switching shows exactly what would change. The agent can preview every change and catch errors before they affect the running system.

**Conflict detection is free.** If two tasks touch conflicting configuration, `nix build` fails at evaluation time rather than silently producing a broken system.

**Reproducibility is free.** "Set up my new laptop like my old one" = clone the config repo and rebuild.

## Platform Tiers

The agent adapts to what's available, always preferring the most declarative option.

### Tier 1: NixOS

Full declarative control over the entire system.

| Scope | Mechanism |
|-------|-----------|
| Kernel, boot, filesystems | `configuration.nix` |
| System services | `systemd.services` in NixOS config |
| System packages | `environment.systemPackages` |
| Networking, firewall | NixOS networking modules |
| User environment | Home Manager |
| Desktop / appearance | Home Manager + NixOS display manager config |

Nearly everything the user asks for can go through Nix. Imperative fallback is rarely needed.

### Tier 2: Non-NixOS Linux + Nix + Home Manager

Nix is installed as a package manager alongside the distro's native package manager. Home Manager manages the user environment.

| Scope | Mechanism |
|-------|-----------|
| User packages | `home.packages` |
| Dotfiles / app config | `home.file`, dedicated HM modules |
| User services | `systemd.user.services` via HM |
| Shell config | HM shell modules (zsh, bash, fish) |
| Desktop / appearance | HM dconf module (GNOME), other DE modules |
| **System packages** | **Imperative fallback** (apt/dnf/pacman) |
| **System services** | **Imperative fallback** (systemctl) |
| **Kernel / boot** | **Imperative fallback** |

The agent handles a large portion of daily configuration declaratively. System-level changes fall back to imperative mode with logging.

### Tier 3: macOS + nix-darwin + Home Manager

`nix-darwin` provides NixOS-style configuration for macOS system settings.

| Scope | Mechanism |
|-------|-----------|
| System defaults | `defaults` module in nix-darwin |
| System packages | nix packages or Homebrew via nix-darwin |
| Launch daemons | `launchd.daemons` in nix-darwin |
| User environment | Home Manager |
| User launch agents | `launchd.agents` via HM or nix-darwin |
| Application config | `home.file` for dotfiles |
| **Some system settings** | **Imperative fallback** (AppleScript, `defaults write` for uncovered domains) |

macOS coverage is good and improving. The agent uses nix-darwin modules where available and falls back to `defaults write` / `osascript` for gaps.

**macOS-specific constraints:**
- **SIP (System Integrity Protection)** is a hard boundary. The agent cannot modify SIP-protected paths or system binaries. nix-darwin works within SIP by managing configuration through `/etc/` symlinks and the Nix store.
- **Rollback** works via `darwin-rebuild switch --rollback` (reverts to the previous nix-darwin generation). Unlike NixOS, there is no boot menu fallback — the system must be bootable for rollback to work. Git revert + rebuild is the primary rollback mechanism.
- **Automations** use `launchd` agents/daemons (not systemd). Home Manager and nix-darwin both support declaring launchd services in Nix modules.

Nix is a hard requirement. The agent does not operate without it.

## Config Repository Structure

The config repo is a root-owned mono repo containing system and per-user configuration in a single Nix flake. The repo location is platform-specific:

- **NixOS / Linux**: `/etc/lares/`
- **macOS**: `/Library/Lares/`

Home Manager is integrated into the system module so a single rebuild applies everything atomically.

```nix
# flake.nix (NixOS example)
{
  description = "Lares-managed system configuration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, home-manager, ... }: {
    nixosConfigurations.myhostname = nixpkgs.lib.nixosSystem {
      modules = [
        ./system/default.nix
        home-manager.nixosModules.home-manager
        {
          home-manager.useGlobalPkgs = true;
          home-manager.useUserPackages = true;
          home-manager.users.alice = import ./users/alice/default.nix;
          home-manager.users.bob = import ./users/bob/default.nix;
        }
      ];
    };
  };
}
```

Directory layout:

```
flake.nix
system/                           # System-level config
  default.nix
users/                            # Per-user Home Manager configs
  alice/
    default.nix
    packages.nix
    shell.nix
  bob/
    default.nix
    packages.nix
    shell.nix
lares/
  tasks/                          # Per-user task journals
    alice/
    bob/
  automations/
    shared/                       # System-wide automation modules
      default.nix
    alice/                        # Per-user automation modules
      default.nix
    bob/
      default.nix
```

### Agent-managed automations as Nix modules

Automations are Nix modules scoped per user. System-wide automations live in `lares/automations/shared/`, per-user automations in `lares/automations/<username>/`.

Example — a per-user file watcher on NixOS/Linux (systemd + inotify):

```nix
# lares/automations/alice/watch-inbox-convert.nix
#
# Task: #12
# Prompt: "when I put files in ~/inbox, convert to jpeg and upload to S3"
# Created: 2026-03-01
{ pkgs, ... }:
{
  systemd.user.services.lares-watch-inbox = {
    Unit.Description = "Convert images in ~/inbox to JPEG and upload";
    Install.WantedBy = [ "default.target" ];
    Service = {
      ExecStart = "${pkgs.writeShellScript "watch-inbox" ''
        ${pkgs.inotify-tools}/bin/inotifywait -m -e create ~/inbox |
        while read dir event file; do
          ${pkgs.imagemagick}/bin/convert "$dir/$file" "/tmp/$file.jpg"
          ${pkgs.awscli2}/bin/aws s3 cp "/tmp/$file.jpg" s3://my-bucket/
        done
      ''}";
      Restart = "always";
    };
  };
}
```

The same automation on macOS (launchd + fswatch):

```nix
# lares/automations/alice/watch-inbox-convert.nix
#
# Task: #12
# macOS version — uses launchd and fswatch instead of systemd and inotify
{ pkgs, ... }:
{
  launchd.agents.lares-watch-inbox = {
    enable = true;
    config = {
      Label = "com.lares.watch-inbox";
      ProgramArguments = [
        "${pkgs.writeShellScript "watch-inbox" ''
          ${pkgs.fswatch}/bin/fswatch -0 ~/inbox |
          while IFS= read -r -d "" file; do
            ${pkgs.imagemagick}/bin/convert "$file" "/tmp/$(basename "$file").jpg"
            ${pkgs.awscli2}/bin/aws s3 cp "/tmp/$(basename "$file").jpg" s3://my-bucket/
          done
        ''}"
      ];
      RunAtLoad = true;
      KeepAlive = true;
    };
  };
}
```

Each user's Home Manager config imports their automations (and shared automations):

```nix
# users/alice/default.nix
{ ... }:
{
  imports = [
    ./packages.nix
    ./shell.nix
  ] ++ (import ../../lares/automations/shared/default.nix)
    ++ (import ../../lares/automations/alice/default.nix);
}
```

This means automations are:
- Version-controlled (git)
- Rollbackable (remove the file, rebuild)
- Auditable (commit history)
- Portable (clone to another machine)
- Scoped per user (the daemon only creates automations in the requesting user's directory)

## The Agent's Nix Workflow

For every configuration change:

1. **Evaluate**: determine if a Nix module/option exists for this change
2. **Edit**: modify the appropriate `.nix` file
3. **Commit**: `git commit` with task reference and description
4. **Build**: `nix build` to verify (catches syntax errors, type errors, conflicts)
5. **Switch**: `nixos-rebuild switch` or `home-manager switch`
6. **Verify**: confirm the change took effect (query the system state)

If step 4 fails, the agent reads the error, fixes the config, and retries. The git history preserves the failed attempt for learning.

## Bootstrapping

Nix must be installed before Lares can operate. The user installs Nix and Home Manager (and nix-darwin on macOS) first, then runs:

```sh
sudo lares init
```

This detects the platform tier, scaffolds the config repo at the platform-appropriate location, creates the daemon config (`/etc/lares/lares.toml`), prompts for the API key, and makes an initial git commit. See [Getting Started](./getting-started.md) for details.

## Dealing with Nix Limitations

**Missing modules**: When no HM/NixOS module exists for an app, the agent uses `home.file` to declaratively manage the app's config files directly. The agent knows the config format from its training data.

**Rebuild latency**: Config-only changes typically rebuild in seconds. For changes the user expects to see immediately, the agent can apply imperatively for instant feedback and reconcile into Nix config in the same operation.

**Nix language complexity**: The agent generates Nix code using its pretrained knowledge. For complex expressions, it can use `nix eval` to validate before committing. Over time, the agent learns patterns from the repo's existing style.
