# Goals

## The problem

A computer is a tool for solving tasks, and it should complement the user's way of working. macOS and Windows are user-friendly but lock users into a specific workflow. Linux offers far more flexibility — any aspect of the system can be customized to match how someone actually works — but the system administration skills and time investment required make it impractical for most people. Even the friendliest Linux distros eventually send you to the terminal just to use the system normally, let alone to shape it around your workflows.

This means the people who would benefit most from Linux's flexibility — those with specific, non-standard ways of working — are the least likely to be able to use it.

## The bet

AI changes this equation. A novice user could describe what they want in natural language and have an agent configure their system accordingly — hyper-customizing a Linux machine to their needs without ever knowing the terminal exists. What previously required deep system administration knowledge becomes a conversation.

But this creates a new problem: if the user doesn't know how to configure the system themselves, an agent making arbitrary edits will quickly produce a broken, unrecoverable mess. Declarative configuration with rollback is what makes agent-driven system management safe. If the agent breaks something, even a novice can roll back and try again. The safety net enables the autonomy.

This is why Nix is a hard requirement, not a nice-to-have. Nix provides:
- **Atomic changes** — a rebuild either succeeds completely or fails completely
- **Automatic rollback** — every change creates a new generation, previous states are always available
- **Full audit trail** — git history + task journals show exactly what changed and why
- **Reproducibility** — the configuration fully determines system state

## What Lares should enable

### For novice users

A Linux system that is more accessible than macOS or Windows because it adapts to the user rather than forcing the user to adapt to it.

- "Nothing happens when I click a .docx document" → the agent installs LibreOffice, sets up file associations, and verifies it works
- "I want my photos automatically sorted by date into folders" → the agent creates a file watcher automation
- "My wifi keeps dropping" → the agent investigates logs, identifies the issue, and applies a fix
- The user never sees a terminal, never edits a config file, never needs to know what a package manager is
- When something breaks, rolling back is a single action — not a troubleshooting session

### For expert users

A time-saving assistant that handles the tedious parts of system administration with precision.

- "Set up a tiling window manager with the following workflow..." → the agent configures it exactly as specified
- "Migrate my shell config from bash to zsh, keep my aliases and functions" → the agent reads the existing config, translates it, and verifies nothing was lost
- "Set up a development environment for this Rust project with direnv" → the agent creates the flake, shell config, and integration
- The expert gives precise instructions and the agent executes them correctly, saving hours of manual configuration

### For macOS users

A partial but valuable replication of the NixOS experience. macOS is locked down compared to NixOS — not everything can be managed declaratively — but the core value still applies:

- Declarative management of system defaults, packages, shell configuration, and user environment via nix-darwin + Home Manager
- The same git-tracked, rollbackable workflow for everything nix-darwin covers
- Honest about the boundaries — the agent knows what it can manage declaratively and what requires imperative fallback

### Across all users

- **Troubleshooting** — the agent can investigate issues by reading logs, checking recent changes, and correlating symptoms with its task history. A novice user gets expert-level debugging without the expertise.
- **Change tracking** — every change is recorded with context (what was asked, what was done, why). This makes it possible to understand what happened weeks later and to roll back specific changes.
- **Proactive monitoring** (future) — the automation and scheduler system can evolve to watch for issues (failed services, disk space, security updates) and alert or auto-fix, within the user's trust boundaries.

## Design principles

1. **Safety enables autonomy** — the more reversible and auditable the agent's actions are, the more trust users can grant. Nix's declarative model is the foundation. This means the default trust tier should let the agent act freely on declarative changes (Tier 1), not gate every action behind an approval prompt (Tier 0). A novice can't meaningfully review a Nix config diff — the real safety is rollback, not approval.
2. **The agent adapts to the user, not the reverse** — a novice describes outcomes ("I want this to work"), an expert describes implementations ("configure this module with these options"). The agent meets users where they are. This applies to communication style too: a novice gets "I installed LibreOffice so you can open Word documents — try double-clicking the file again," while an expert gets "Added `pkgs.libreoffice` to `home.packages` in `users/alice/packages.nix`, rebuilt, verified MIME associations."
3. **No hidden state** — everything the agent does is visible in git history, task journals, and nix generations. The user (or a support person) can always understand what happened.
4. **Graceful degradation** — on macOS, the agent manages what it can declaratively and is transparent about what it can't. On NixOS, nearly everything is declarative.
5. **The interface is not the product** — the CLI is the first interface, but the value is in the agent + nix backend. Future interfaces (GUI chat, file manager integration, system tray, notification-driven) are presentation layers over the same daemon.

## Current status

The current implementation is CLI-first and developer-oriented. This is deliberate — the agent + nix backend is the core value, and the CLI is the fastest way to validate it. But the novice user vision described above requires layers that don't exist yet:

- **Installer / packaging** — today, setup requires a terminal (installing Nix, running `sudo lares init`, starting the daemon). A novice-accessible path would be a NixOS ISO with Lares pre-installed, a `.deb`/`.rpm` package that handles Nix installation, or a macOS `.pkg` installer. This is future work.
- **GUI interfaces** — the daemon already communicates over a message-based protocol (see [Architecture](./architecture.md)), so CLI, GUI chat, file manager integration, and notification-driven interfaces are all presentation layers over the same backend. The CLI ships first; GUI interfaces come later.
- **Conversational rollback** — the user should be able to say "undo that" and have the agent roll back. The mechanical primitives (Nix generations, git revert) exist; the conversational UX is designed (see [Architecture: Rollback](./architecture.md)) but not yet implemented.

The path from here to the novice vision: get the agent + nix backend working correctly (CLI), then layer on packaging and GUI. The architecture is designed so that adding interfaces does not require changing the backend.
