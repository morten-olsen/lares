# Client Protocol

The daemon communicates with clients over a Unix socket using a simple message-based protocol. Any program that can connect to a Unix socket and exchange JSON messages can be a Lares client — CLI, GUI, system tray app, file manager plugin, etc.

**Source of truth**: `crates/lares-protocol/src/types.rs`. The types below are derived from the Rust definitions. If this document and the code disagree, the code wins.

## Transport

- **Socket**: Unix domain socket (path configured in daemon config, default `/tmp/lares-{uid}/lares.sock`)
- **Framing**: Length-delimited — each message is a 4-byte big-endian length prefix followed by that many bytes of JSON
- **Encoding**: UTF-8 JSON, one message per frame
- **Direction**: Full duplex — both sides can send messages at any time after connection

The length-delimited framing is handled by `tokio-util`'s `LengthDelimitedCodec` in the Rust implementation. For non-Rust clients, the framing is trivial: read 4 bytes as a big-endian u32, then read that many bytes as JSON.

## Conversation flow

A typical session:

```
Client                                    Daemon
  │                                         │
  │─── Prompt ────────────────────────────>│
  │                                         │
  │<── TaskStarted ────────────────────────│
  │<── AgentText (streaming) ──────────────│
  │<── ToolExecuting ──────────────────────│
  │<── ToolResult ─────────────────────────│
  │<── AgentText ──────────────────────────│
  │                                         │
  │<── ApprovalRequest ────────────────────│  (agent wants to mutate)
  │─── ApprovalResponse ──────────────────>│  (user approves/rejects)
  │                                         │
  │<── ToolExecuting ──────────────────────│
  │<── ToolResult ─────────────────────────│
  │<── AgentText ──────────────────────────│
  │<── TaskCompleted ──────────────────────│
  │                                         │
  │                    (connection can close or send another Prompt)
```

The client sends a `Prompt` to start a task. The daemon streams events back. If the agent needs approval or has a question, the daemon sends a request and waits for the client's response before continuing.

## Client → Daemon messages

All messages have a `"type"` field (serde internally-tagged enum).

### Prompt

Start a new task or resume an existing one.

```json
{
  "type": "Prompt",
  "text": "enable dark mode",
  "task_id": null
}
```

| Field | Type | Description |
|-------|------|-------------|
| `text` | string | The natural language prompt |
| `task_id` | string \| null | If set, resume this existing task instead of creating a new one |

### ApprovalResponse

Respond to an `ApprovalRequest` from the daemon.

```json
{
  "type": "ApprovalResponse",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "approved": true
}
```

| Field | Type | Description |
|-------|------|-------------|
| `request_id` | UUID string | Must match the `request_id` from the `ApprovalRequest` |
| `approved` | boolean | `true` to approve, `false` to reject |

### UserReply

Answer a `Question` from the agent.

```json
{
  "type": "UserReply",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "text": "nightly, and I use Helix"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `request_id` | UUID string | Must match the `request_id` from the `Question` |
| `text` | string | The user's answer |

### Cancel

Abort the current task.

```json
{
  "type": "Cancel"
}
```

## Daemon → Client events

All events have a `"type"` field.

### TaskStarted

A new task was created for this prompt.

```json
{
  "type": "TaskStarted",
  "task_id": "003"
}
```

### AgentText

Streaming text from the agent. A single response may arrive as multiple `AgentText` events.

```json
{
  "type": "AgentText",
  "text": "I'll enable dark mode in your nix-darwin configuration."
}
```

### ToolExecuting

The agent is about to use a tool.

```json
{
  "type": "ToolExecuting",
  "tool_name": "edit_file",
  "summary": "Edit system/default.nix to enable dark mode"
}
```

### ToolResult

A tool finished executing.

```json
{
  "type": "ToolResult",
  "tool_name": "edit_file",
  "summary": "Updated system/default.nix",
  "success": true
}
```

### ApprovalRequest

The agent proposes a mutation and needs the user's approval before proceeding. The daemon blocks until the client sends an `ApprovalResponse` with the matching `request_id`.

```json
{
  "type": "ApprovalRequest",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "action": {
    "action_type": "FileEdit",
    "path": "/Library/Lares/system/default.nix",
    "description": "Enable dark mode in nix-darwin",
    "new_content": "{ config, pkgs, ... }:\n{\n  system.defaults.NSGlobalDomain.AppleInterfaceStyle = \"Dark\";\n}\n"
  }
}
```

The `action` field is a `ProposedAction` (see below).

### Question

The agent needs more information from the user. The daemon blocks until the client sends a `UserReply`.

```json
{
  "type": "Question",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "text": "Do you want stable or nightly Rust?"
}
```

### TaskCompleted

The task finished successfully.

```json
{
  "type": "TaskCompleted",
  "task_id": "003",
  "summary": "Enabled dark mode and rebuilt. The change is active."
}
```

### TaskFailed

The task failed.

```json
{
  "type": "TaskFailed",
  "task_id": "003",
  "error": "nix build failed: attribute 'darq' not found"
}
```

### Error

A protocol-level or daemon error (not a task failure).

```json
{
  "type": "Error",
  "message": "API key not set"
}
```

## ProposedAction

Nested inside `ApprovalRequest`. Has an `"action_type"` discriminator field.

### FileEdit

```json
{
  "action_type": "FileEdit",
  "path": "/Library/Lares/users/alice/packages.nix",
  "description": "Add ripgrep to user packages",
  "new_content": "{ pkgs, ... }:\n{\n  home.packages = with pkgs; [ ripgrep ];\n}\n"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Absolute path to the file being edited |
| `description` | string | Human-readable summary of the change |
| `new_content` | string | The complete new file content |

### RunCommand

```json
{
  "action_type": "RunCommand",
  "command": "darwin-rebuild switch --flake .",
  "working_dir": "/Library/Lares",
  "description": "Rebuild system configuration"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `command` | string | The shell command to execute |
| `working_dir` | string \| null | Working directory, or null for daemon default |
| `description` | string | Human-readable summary of what this command does |

## Implementing a new client

A minimal client needs to:

1. Connect to the Unix socket
2. Send a length-delimited `Prompt` message
3. Read events in a loop, handling:
   - `AgentText` — display to user
   - `ApprovalRequest` — present the proposed action, send back `ApprovalResponse`
   - `Question` — present the question, send back `UserReply`
   - `TaskCompleted` / `TaskFailed` — display result, optionally disconnect
4. Optionally send `Cancel` to abort

The `ToolExecuting` and `ToolResult` events are informational — a minimal client can ignore them. A richer client can use them to show progress indicators.

### Length-delimited framing in pseudocode

```
function send(socket, message):
    json = json_encode(message)
    bytes = utf8_encode(json)
    socket.write(big_endian_u32(bytes.length))
    socket.write(bytes)

function receive(socket):
    length = big_endian_u32(socket.read(4))
    bytes = socket.read(length)
    return json_decode(utf8_decode(bytes))
```

### Example: Python client skeleton

```python
import socket
import struct
import json

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect("/tmp/lares-501/lares.sock")

def send_msg(msg):
    data = json.dumps(msg).encode()
    sock.sendall(struct.pack(">I", len(data)) + data)

def recv_msg():
    length_bytes = sock.recv(4)
    length = struct.unpack(">I", length_bytes)[0]
    data = b""
    while len(data) < length:
        data += sock.recv(length - len(data))
    return json.loads(data)

# Send a prompt
send_msg({"type": "Prompt", "text": "what OS am I running?", "task_id": None})

# Read events
while True:
    event = recv_msg()
    if event["type"] == "AgentText":
        print(event["text"], end="")
    elif event["type"] == "ApprovalRequest":
        answer = input(f"\nApprove: {event['action']['description']}? [y/n] ")
        send_msg({"type": "ApprovalResponse",
                   "request_id": event["request_id"],
                   "approved": answer.lower() == "y"})
    elif event["type"] in ("TaskCompleted", "TaskFailed"):
        print(f"\n[{event['type']}] {event.get('summary', event.get('error', ''))}")
        break
```
