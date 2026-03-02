# Agent Design

The agent is the LLM-powered reasoning core of Lares. It doesn't contain hardcoded knowledge about how systems work — it relies on the model's pretrained understanding of Linux, macOS, Nix, and system administration. Lares provides the infrastructure: execution, context, safety, and persistence.

## Agent Primitives

The agent has five fundamental capabilities:

### 1. Execute

Run commands on the system or modify Nix configuration.

- **Declarative execution**: edit `.nix` files → git commit → rebuild
- **Imperative execution**: run a shell command with captured output
- The agent decides which path based on the current platform tier and whether a Nix module exists for the change
- All execution goes through the daemon's security policy

### 2. Observe

Gather information about the system without changing it.

- Read files, query system state, check logs
- Always permitted (no approval needed for read-only operations)
- Used for investigation, verification, and context building

### 3. Remember

Write to the task journal and reference past tasks.

- Append observations, hypotheses, actions, and results to the current task
- Query past tasks for relevant history
- The journal is the agent's working memory — it re-reads it when resuming a task

### 4. Schedule

Set triggers that will wake the agent later.

- Time-based: "check back in 3 days"
- Event-based: "notify me when a file appears in this directory"
- Condition-based: "alert me if this log pattern appears again"
- Each trigger is associated with a task, so the agent has full context when it wakes

### 5. Ask

Escalate to the user when uncertain.

- Request approval for risky actions
- Ask clarifying questions
- Report progress or results
- The agent should prefer asking to guessing when the stakes are high

## Context Enrichment

Before the LLM sees a prompt, the system gathers context. This happens in stages:

### Stage 1: Always gathered (cheap)

- OS, architecture, kernel version
- Current Nix tier (NixOS / nix-darwin+HM / HM only / none)
- Current Nix generation and recent config commits
- Active tasks and automations (summary)
- User's CWD and active window (if available)

### Stage 2: Prompt-dependent (on demand)

Based on what the prompt seems to be about, gather:

- Relevant system logs (sleep issues → `pmset` log, power events)
- Relevant config sections (networking prompt → current network config)
- Relevant past tasks (similar prompts or related subsystems)
- Installed packages, running services (if the prompt involves software)
- Hardware info (if the prompt involves devices, performance, power)

### Stage 3: Agent-requested (during reasoning)

The agent can request additional context mid-reasoning:

- "Show me the output of `journalctl -u bluetooth`"
- "What's in `~/.config/sway/config`?"
- "What Nix options are available for `services.openssh`?"

This is just the observe primitive used during planning.

## Prompt Construction

The LLM receives a structured prompt:

```
[system instructions]
You are Lares, a system management agent. You manage this machine by editing
Nix configuration and occasionally running imperative commands.

[current context]
Platform: NixOS 24.05, x86_64
Desktop: GNOME 45 (Wayland)
Nix tier: full NixOS
Recent config changes: (last 5 commits)
Active tasks: #7 (sleep troubleshooting, waiting), #12 (inbox watcher, active)
User context: terminal at ~/Documents

[task context]  (if resuming a task)
Task #7 journal: (full journal content)

[user prompt]
"my computer still won't wake from sleep"
```

The context window budget matters. For long-running tasks with extensive journals, older entries may need summarization. The agent can compress its own journal: "entries 1-15 summary: investigated power settings, changed hibernatemode to 3, bluetooth wake still causing issues."

## Adaptive Communication

The agent adapts how it communicates based on who it's talking to. A novice user and an expert user may ask for the same thing, but the agent's response style should differ:

**Novice** — "Nothing happens when I click a .docx document":
> I installed LibreOffice so you can open Word documents. Try double-clicking the file again — it should open now. If you want to undo this, just say "undo that."

**Expert** — "Add libreoffice to my packages":
> Added `pkgs.libreoffice` to `users/alice/packages.nix`, rebuilt. MIME associations for `.docx`/`.xlsx`/`.pptx` should be set by the desktop file. Committed as `abc123f`.

The difference is in the signal, not the action. Both users get the same declarative change, the same commit, the same rollback capability. But the novice gets outcome-focused language ("you can open Word documents") while the expert gets implementation details ("added `pkgs.libreoffice` to `users/alice/packages.nix`").

How the agent knows: a per-user `experience_level` setting (novice / intermediate / expert), configured at `lares init` time or changed later ("talk to me like I'm a beginner" / "give me the technical details"). The agent includes this in its system prompt so the LLM adapts its tone and level of detail accordingly.

This is not about dumbing down — the agent does the same amount of work either way. It's about matching the explanation to the audience.

## Planning and Execution

The agent follows a loop:

```
1. Read context + prompt
2. Reason about what to do
3. Propose action(s)
4. [if approval required] ask user
5. Execute
6. Observe result
7. Log to journal
8. Decide: done? need more steps? schedule follow-up?
9. If more steps, goto 2
```

The agent can take multiple actions in a single turn (edit multiple nix files, run several diagnostic commands). It should batch related changes into a single commit when they're part of the same logical change.

## Knowledge Sources

The agent's primary knowledge comes from the LLM's pretraining. For Nix-specific information, additional sources help:

- **`nixos-option`**: query available NixOS module options and their types
- **`home-manager option`**: query available Home Manager options
- **`nix search`**: find packages
- **`man` pages**: system command documentation
- **Existing config**: the repo's current `.nix` files show established patterns
- **Git history**: how the agent (or user) has solved similar problems before

The agent should query these as needed rather than relying solely on its training data, especially for option names and package availability which change across Nixpkgs versions.

## Error Handling

When things go wrong:

**Nix build failure**: Read the error message, fix the config, retry. This is the most common error and usually means a syntax error or type mismatch. Log the failure and fix in the journal.

**Rebuild failure**: The system is still on the previous generation — nothing is broken. Diagnose and fix, or rollback the git commit and report to the user.

**Imperative command failure**: Log the error, check if anything was partially applied, attempt to restore previous state using captured before-state. Report to user.

**Unexpected system state**: The agent finds something it didn't expect (a config file it didn't write, a service it doesn't recognize). It should investigate rather than overwrite. Ask the user if uncertain.

**Repeated failures**: If the agent has tried 3+ approaches to solve a problem, it should escalate to the user with a summary of what it tried and why it failed, rather than continuing to flail.

## Multi-turn and Conversational

Not every interaction is a single command. The user might have a conversation:

```
User: "I want to set up a development environment for Rust"
Agent: "I'll add the Rust toolchain and common tools. Do you want stable or
        nightly? And do you use VS Code or another editor?"
User: "Nightly, and I use Helix"
Agent: (edits packages.nix to add rustup, rust-analyzer, helix, etc.)
```

The agent maintains conversational context within a session and logs the full exchange in the task journal. If the user comes back days later and says "add Python to my dev setup too," the agent can find the dev environment task and extend it.
