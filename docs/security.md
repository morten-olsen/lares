# Security

Lares has significant power over the host system. The security model is designed around the principle that **safety and auditability enable autonomy** — the more visible and reversible the agent's actions are, the more trust the user can grant.

## Trust Tiers

The user configures a trust level that determines what the agent can do without asking.

The default trust tier depends on context. For novice users — who rely on the agent to act on their behalf and depend on Nix's rollback as their safety net — **Tier 1 (Declarative autonomy)** is the right default. An approval prompt for every nix config edit defeats the purpose: the novice doesn't know whether the proposed change is correct, so asking them to approve it provides false security rather than real safety. The real safety comes from Nix's atomicity and rollback.

For expert users who want to review every change, **Tier 0 (Observer)** is appropriate. `lares init` asks the user which default to start with.

### Tier 0: Observer

The agent can only read system state and propose changes. Every mutation requires explicit approval. Appropriate for experts who want to review and learn from every change.

- Read files, query system state, check logs: **auto-approved**
- Edit Nix config files: **requires approval**
- Rebuild/switch: **requires approval**
- Run imperative commands: **requires approval**
- Schedule triggers: **requires approval**

### Tier 1: Declarative autonomy

The agent can freely modify Nix configuration and rebuild, since these changes are inherently reversible via rollback. Imperative actions still require approval. This is the recommended default for most users — the safety net is Nix, not the approval prompt.

- Read and observe: **auto-approved**
- Edit Nix config + rebuild: **auto-approved**
- Imperative read-only commands (`ls`, `ps`, `cat`): **auto-approved**
- Imperative mutations: **requires approval**
- Destructive operations (`rm`, `kill`, etc.): **requires approval**

### Tier 2: Full autonomy

The agent can take any action, with logging. The user trusts the agent and reviews the audit log rather than approving individual actions.

- Everything: **auto-approved and logged**
- Destructive operations: **auto-approved with prominent journal entry**

The user can override at any time ("from now on, ask before changing network settings"). Trust can also be scoped by domain ("autonomous for packages, ask for services").

## Privilege Model

The daemon runs as root (a system service managed by systemd/launchd). This eliminates the need for privilege escalation — the daemon already has the authority to make system-level changes. Instead, it **drops privileges** to execute user-level tasks.

### Why root

Escalating from user to root is inherently fragile:
- `sudo` requires credential caching or NOPASSWD rules
- polkit policies are complex and vary across distros
- Interactive auth prompts break automated workflows
- Each escalation mechanism is a potential failure point

Running as root and dropping down is the standard pattern for system services (sshd, cron, docker). The daemon has the authority it needs and delegates carefully.

### Execution contexts

The daemon runs commands in one of two contexts:

**Root context** — for system-level operations:
- `nixos-rebuild switch`
- System service management (`systemctl start/stop/restart`)
- System package operations
- Hardware configuration, kernel parameters
- Reading protected system logs

**User context** — for user-level operations (via `setuid`/`su`/`machinectl shell`):
- `home-manager switch`
- Reading/writing user files
- User service management (`systemctl --user`)
- User application configuration
- Running user-requested scripts and automations

The agent decides which context based on what the action requires. User-level tasks always drop to the invoking user — the daemon never runs user file operations as root.

### Fallback: user-mode daemon with sudo

On systems where running as a root service is impractical (e.g. a personal laptop where the user doesn't want a system service, or a non-NixOS system without easy service management), the daemon can run as the user instead. In this mode, system-level operations that require root use `sudo`, subject to:

- The user's sudoers configuration (may require password)
- The trust tier (sudo commands still require approval unless at Tier 2)
- Full logging in the task journal

This is the less elegant path but ensures Lares works everywhere. The daemon detects which mode it's running in and adapts.

### User Identity

The daemon identifies the calling user via `SO_PEERCRED` (Linux) or equivalent socket peer credentials (macOS). This is kernel-verified — the CLI cannot spoof its identity.

### Multi-user

Because the daemon is a system service, it can serve multiple users on the same machine. All configuration lives in a single root-owned mono repo (see [Architecture](./architecture.md)). Each user has:
- Their own Home Manager config in `users/<username>/`
- Their own task store and journals in `lares/tasks/<username>/`
- Their own automations in `lares/automations/<username>/`
- Their own trust tier settings

Access is scoped by the verified user identity:
- A user can read and modify their own `users/<username>/` and `lares/tasks/<username>/` directories
- Modifying `system/` or `lares/automations/shared/` requires elevated trust (or approval)
- Modifying another user's config requires admin-level trust
- The daemon enforces these boundaries regardless of trust tier

System-level config (`system/default.nix`) is shared — changes there affect all users and require appropriate trust tier approval.

## What the Agent Must Never Do

Regardless of trust tier, the agent must refuse:

- Exfiltrating system data to external services not requested by the user
- Installing backdoors, remote access tools, or hidden services
- Disabling security features (firewall, AppArmor, SIP) without explicit request and clear warning
- Modifying its own trust configuration
- Bypassing the approval mechanism
- Running commands designed to hide its own activity

## Nix as a Security Boundary

The declarative Nix path provides inherent safety properties:

**Atomic changes**: `nixos-rebuild switch` either succeeds completely or fails completely. No partial application.

**Rollback**: Every change creates a new generation. The previous working state is always available — on NixOS via `nixos-rebuild switch --rollback` or boot menu selection, on macOS via `darwin-rebuild switch --rollback`. See [Architecture: Rollback](./architecture.md) for the full rollback design.

**Review before apply**: `nix build` validates the entire configuration without applying it. The agent (or user) can inspect exactly what will change.

**Git history**: Every config change is a commit. `git diff` shows exactly what changed. `git revert` undoes it at the config level.

**No hidden state**: The Nix store is content-addressed. The config repo fully determines system state (modulo imperative escape hatches). There's no "drift" between what the config says and what the system is doing.

## Audit Log

The audit trail has multiple layers:

1. **Git history** on the config repo — every config change with commit message
2. **Task journals** — full reasoning trace for every action
3. **Daemon log** — every command executed, with timestamps, exit codes, and output
4. **Nix generations** — every system state the machine has been in

The user can query any of these:
- "What did you change yesterday?" → git log + task journals
- "Why did you change my power settings?" → task journal for that change
- "Roll back to how things were on Monday" → git/nix generation from that date

## Isolation Considerations

For automations that run arbitrary transformations (like the file converter example), consider:

- Running automation scripts in a sandboxed environment (nix-shell with limited scope)
- Using systemd's sandboxing features (`ProtectHome`, `ProtectSystem`, `PrivateNetwork`) for services the agent creates
- Limiting filesystem access to only the directories the automation needs

The agent should apply the principle of least privilege when creating services — a file watcher for `~/inbox` doesn't need access to `~/.ssh`.

## Incident Response

If something goes wrong:

1. The user says "something broke" or the agent detects a problem
2. The agent checks recent config commits and task actions
3. If a recent change is likely the cause, offer immediate rollback
4. If the cause is unclear, open a troubleshooting task
5. Never compound the problem — when in doubt, rollback first, investigate second
