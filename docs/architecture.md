# Architecture

## Overview

Lares is a daemon that mediates between the user and their system. It accepts natural language prompts, enriches them with system context, passes them to an LLM for planning, and executes the resulting changes — preferring declarative Nix configuration over imperative commands.

```
                    ┌─────────────┐
                    │  User Prompt │
                    │  (CLI / UI)  │
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐
                    │   Context    │  system state, task journals,
                    │  Enrichment  │  nix config, git history
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐
                    │    Agent     │  LLM reasons about what to change
                    │   (Plan)     │  outputs: config edits or commands
                    └──────┬──────┘
                           │
                ┌──────────┼──────────┐
                ▼                     ▼
      ┌─────────────────┐  ┌──────────────────┐
      │  Declarative     │  │  Imperative      │
      │  (Nix Config)    │  │  (Fallback)      │
      │                  │  │                  │
      │ edit .nix file   │  │ capture before   │
      │ git commit       │  │ run command      │
      │ rebuild / switch │  │ capture after    │
      │ verify           │  │ record undo      │
      └────────┬─────────┘  └────────┬─────────┘
               │                     │
               └──────────┬──────────┘
                          │
                   ┌──────▼──────┐
                   │ Task Journal │  log result, update task,
                   │   + Store    │  schedule follow-ups
                   └─────────────┘
```

## Components

### Daemon

A root-owned system service (systemd unit / launchd daemon). Responsibilities:

- Accept prompts from CLI, UI, or scheduled triggers
- Manage the agent loop (context → plan → execute → log)
- Own the scheduler (triggers, follow-ups, automations)
- Enforce the security policy (approval gates, trust tiers)
- Execute system-level operations as root
- Drop privileges to the invoking user for user-level operations

The daemon is the only component that executes commands on the system. The LLM never has direct access — it proposes actions, and the daemon executes them within the security policy. See [Security](./security.md) for the full privilege model.

### Daemon Configuration

The daemon reads its configuration from `lares.toml` at the root of the config repo. This is a system-level file (root-owned, gitignored). It contains API credentials, model settings, and the socket path. `lares init` creates this file and prompts for the API key interactively.

### Config Repository

The Nix configuration repo is the source of truth for system state. It is a root-owned mono repo containing both system-level and per-user configuration, plus the daemon config. The repo location is platform-specific:

- **NixOS / Linux**: `/etc/lares/`
- **macOS**: `/Library/Lares/`

Structure:

```
lares.toml                      # Daemon config (API key, model — gitignored)
flake.nix
flake.lock

system/                         # System-level config (NixOS or nix-darwin)
  default.nix
  hardware.nix                  # NixOS only
  networking.nix
  services.nix

users/<username>/               # Per-user Home Manager config
  default.nix
  shell.nix
  packages.nix

lares/
  tasks/<username>/             # Per-user task journals (markdown)
  automations/shared/           # System-wide automation modules
  automations/<username>/       # Per-user automation modules
```

Adding a user means creating their directory under `users/` and wiring them into the flake's system module (`home-manager.users.<name> = import ./users/<name>/default.nix`). The daemon identifies the calling user via `SO_PEERCRED` on the Unix socket and scopes access accordingly — a user can modify their own `users/<username>/` and `lares/tasks/<username>/` directories but not other users' or `system/` (unless granted elevated trust).

Every change the agent makes is a git commit with a message referencing the task that caused it. The git history IS the audit log.

### Context Resolver

Gathers information the agent needs to reason about a prompt. Runs before the LLM sees the prompt. Sources:

- **System state**: OS, DE, hardware, running services, installed packages
- **Nix config state**: current generation, pending changes, available options
- **Git history**: recent config changes, what the agent has done before
- **Task state**: active tasks, their journals, active automations
- **User context**: CWD, active window/folder, clipboard (if permitted)
- **System logs**: journalctl (Linux), `log show` / system.log (macOS) entries relevant to the prompt

Context is gathered lazily — the resolver runs cheap queries first and only digs deeper if the prompt warrants it.

### Executor

Runs the actions the agent decides on. Two paths:

**Declarative path** (preferred):
1. Write changes to `.nix` files in the config repo
2. `git add` + `git commit` with task reference
3. `nix build` to verify (dry run)
4. `nixos-rebuild switch` or `home-manager switch` to apply
5. Verify the change took effect

**Imperative path** (fallback):
1. Capture relevant system state before the change
2. Execute the command
3. Capture state after
4. Record the command, before/after state, and inverse command in the task journal

The executor never runs both paths for the same change. The agent decides which path based on what Nix tier is available and whether a Nix module exists for the change.

### Rollback

Rollback is available at multiple levels, from conversational to last-resort:

**Conversational** — the user says "undo that" or "that broke something, roll back." The agent identifies the most recent task's config commits and reverts them:
1. `git revert <commit>` for each commit in the task (in reverse order)
2. Rebuild to apply the reverted config
3. Verify the system is back to the previous state

This is the primary rollback path. It works through the same daemon protocol as any other prompt, so it's available from CLI, GUI, or any future interface.

**Explicit** — `lares rollback` (or "roll back to how things were yesterday"). The agent lists recent generations or config commits and lets the user choose how far back to go. For declarative changes, this is a git revert + rebuild. For imperative changes, the agent replays the inverse commands recorded in the task journal (best-effort — imperative rollback is not guaranteed).

**Last-resort (NixOS only)** — if the system is broken enough that the daemon cannot start, NixOS provides a boot menu with every previous generation. The user selects a working generation at boot time. This is a NixOS feature, not a Lares feature, but it's the safety net that makes agent-driven configuration viable. On macOS, `darwin-rebuild switch --rollback` serves a similar purpose from the terminal (requires the system to be bootable).

**What rollback cannot do:**
- Imperative changes (commands run outside Nix) have recorded inverse commands but no atomicity guarantee. The agent warns when it falls back to imperative mode.
- External side effects (files uploaded, emails sent, services notified) cannot be undone by rolling back config.
- If the user has made manual changes to the config repo between the agent's change and the rollback, the revert may conflict. The agent handles this via standard git conflict resolution.

### Scheduler

Manages triggers that wake the agent:

- **Time-based**: follow up on a task in N days, run weekly maintenance
- **File-based**: watch a directory for new files (inotify / FSEvents)
- **Log-based**: monitor system logs for a pattern
- **Event-based**: system wake, network change, user login

When a trigger fires, the scheduler creates a new agent interaction with the relevant task context preloaded. The agent sees "trigger X fired for task Y" and picks up where it left off.

On NixOS, persistent triggers (file watchers, scheduled tasks) are themselves Nix service definitions — they get the same declarative/rollback guarantees as everything else.

### Task Store

Persists tasks and their journals. See [Task Model](./task-model.md) for details. Stored as files in the config repo (`lares/tasks/<username>/`), scoped per user, committed alongside the config changes they produce.

### Client Protocol

The daemon communicates with clients over a Unix socket using a length-delimited JSON protocol. This is the same protocol regardless of whether the client is a CLI, GUI, or integration — any program that can connect to a Unix socket and exchange JSON messages can be a Lares client. See [Protocol](./protocol.md) for the full specification, JSON schemas, conversation flow, and a guide to implementing new clients.

The protocol types are defined in `crates/lares-protocol/src/types.rs` (source of truth).

### CLI / UI

Entry points for the user:

- **CLI**: `lares "do something"` — primary interface, works over SSH
- **Global hotkey → prompt bar**: floating text input for desktop users
- **Proactive notifications**: the agent surfaces follow-up results or warnings
- **Context menu integration**: right-click → "ask Lares about this"

All entry points use the client protocol over the daemon's Unix socket.
