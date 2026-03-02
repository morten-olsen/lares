# Task Model

Every interaction with Lares is tracked as a task. Tasks are the agent's unit of work, its working memory, and the user's audit trail.

## Task Lifecycle

```
  prompt received
        │
        ▼
  ┌──────────┐     agent investigates, gathers context
  │   open   │────────────────────────────────────┐
  └──────────┘                                    │
        │                                         ▼
        ▼                                  ┌─────────────┐
  ┌──────────────┐    waiting on user,     │ investigating│
  │  in_progress │    external event,      └──────┬──────┘
  └──────┬───────┘    or scheduled                │
         │            follow-up                   │
         │       ┌──────────┐                     │
         ├──────►│ waiting  │◄────────────────────┘
         │       └────┬─────┘
         │            │  trigger fires / user responds
         │            │
         │◄───────────┘
         │
         ├───────────► resolved     (goal achieved)
         └───────────► failed       (gave up or escalated)
```

Tasks can cycle between `in_progress` and `waiting` multiple times — for example, a troubleshooting task that tries a fix, waits a few days to see if it helped, then resumes.

## Task Structure

```
Task:
  id:               unique identifier
  created:          timestamp
  origin_prompt:    the user's original words
  goal:             what success looks like (agent-derived)
  status:           open | investigating | in_progress | waiting | resolved | failed
  journal:          ordered log of entries (see below)
  config_commits:   list of git commit hashes for nix config changes
  rollback_plan:    how to undo everything this task did
  related_tasks:    links to tasks that may be relevant
  triggers:         active triggers waiting to wake this task
```

## The Journal

The journal is the most important part of the task. It's an append-only log of everything the agent did, observed, and decided for this task.

Journal entry types:

```
- observed:     something the agent noticed ("hibernatemode is set to 25")
- hypothesis:   the agent's reasoning ("safe sleep may conflict with this hardware")
- action:       something the agent did ("edited power.nix, set hibernatemode = 3")
- result:       outcome of an action ("rebuild succeeded, change applied")
- error:        something went wrong ("rebuild failed: syntax error on line 12")
- asked:        escalated to user ("is the sleep issue still happening?")
- user_reply:   user responded ("yes, still broken")
- scheduled:    set a trigger ("follow up in 3 days")
- triggered:    a trigger fired ("3 days elapsed, resuming task")
- resolved:     task completed ("user confirms sleep works reliably")
- note:         free-form agent annotation
```

Example journal for a troubleshooting task:

```markdown
# Task #7: Computer won't wake from sleep

**Prompt**: "my computer won't wake from sleep"
**Goal**: Computer reliably wakes from sleep
**Status**: resolved

## Journal

### 2026-03-01 14:30 — opened
User reports computer fails to wake from sleep.

### 2026-03-01 14:30 — observed
Checked power settings: `pmset -g` shows hibernatemode=25 (safe sleep).
System: MacBook Pro M3, macOS 14.2, nix-darwin active.

### 2026-03-01 14:31 — hypothesis
hibernatemode 25 writes RAM to disk before sleeping. On Apple Silicon this
can cause wake failures when the disk image is stale. Mode 3 (regular sleep)
is more reliable for machines that are frequently slept.

### 2026-03-01 14:31 — action
Edited `system/power.nix`: set `power.sleep.hibernatemode = 3`
Commit: a1b2c3d "set hibernatemode to 3 (task #7)"

### 2026-03-01 14:32 — result
nix-darwin rebuild succeeded. Verified: `pmset -g` now shows hibernatemode=3.

### 2026-03-01 14:32 — scheduled
Follow up in 3 days to check if sleep works.

### 2026-03-04 09:00 — triggered
3 days elapsed. Asked user: "Has the sleep issue improved since I changed
the hibernate mode?"

### 2026-03-04 10:15 — user_reply
"It's better but still fails sometimes"

### 2026-03-04 10:15 — observed
Checked system.log around recent sleep events. Found:
`bluetoothd: wake reason: HID` — Bluetooth devices triggering spurious wakes
that then fail to fully resume.

### 2026-03-04 10:16 — action
Edited `system/power.nix`: disabled Bluetooth wake
Commit: d4e5f6a "disable bluetooth wake to fix sleep (task #7)"

### 2026-03-04 10:16 — scheduled
Follow up in 5 days.

### 2026-03-09 09:00 — triggered
Asked user: "How's the sleep issue? Any failures in the past 5 days?"

### 2026-03-09 11:00 — user_reply
"Working perfectly now"

### 2026-03-09 11:00 — resolved
Sleep issue fixed by:
1. Changed hibernatemode from 25 to 3
2. Disabled Bluetooth wake
Config commits: a1b2c3d, d4e5f6a
```

## Automations

Automations are a special case: tasks that create persistent, long-running behavior. When the user says "when files appear in ~/inbox, convert and upload them," the resulting task:

1. Creates a Nix service module (`lares/automations/watch-inbox.nix`)
2. Commits and rebuilds
3. Moves to `resolved` but remains **linked to the automation**

The automation itself is tracked:

```
Automation:
  id:               unique identifier
  created_by_task:  task that created this automation
  description:      "convert images in ~/inbox to JPEG and upload to S3"
  nix_module:       lares/automations/watch-inbox.nix
  status:           active | paused | removed
  execution_log:    recent trigger events and outcomes
```

The user can interact with automations naturally:
- "What automations are running?" → list active automations
- "Pause the inbox watcher" → stop the service, mark paused
- "Change the upload destination to Dropbox" → edit the nix module, rebuild
- "Remove that automation" → delete the module, rebuild, mark removed

## Cross-task Reasoning

Because all tasks and journals live in the same store, the agent can correlate across them:

- "My wifi is slow" — the agent checks if any recent task changed network settings
- "Undo everything from last week" — the agent finds all tasks with config commits in that range
- "Why did you change my power settings?" — the agent retrieves the task that motivated the change

The task store is the agent's long-term memory. When context is needed for a new prompt, relevant past tasks are retrieved and included in the LLM context.

## Storage

Tasks are stored as markdown files in the config repo:

```
lares/tasks/
  007-sleep-troubleshooting.md
  008-dark-mode.md
  012-inbox-watcher.md
```

They are committed to git alongside the config changes they produce. This means:
- Tasks are version-controlled
- They travel with the config (portable to a new machine)
- `git log` shows config changes interleaved with task updates
- The task file IS the journal — no separate database needed
