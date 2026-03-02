use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── CLI → Daemon ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Prompt {
        text: String,
        task_id: Option<String>,
    },
    ApprovalResponse {
        request_id: Uuid,
        approved: bool,
    },
    UserReply {
        request_id: Uuid,
        text: String,
    },
    Cancel,
}

// ── Daemon → CLI ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonEvent {
    TaskStarted {
        task_id: String,
    },
    AgentText {
        text: String,
    },
    ToolExecuting {
        tool_name: String,
        summary: String,
    },
    ToolResult {
        tool_name: String,
        summary: String,
        success: bool,
    },
    ApprovalRequest {
        request_id: Uuid,
        action: ProposedAction,
    },
    Question {
        request_id: Uuid,
        text: String,
    },
    TaskCompleted {
        task_id: String,
        summary: String,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },
    Error {
        message: String,
    },
}

// ── Proposed mutations ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action_type")]
pub enum ProposedAction {
    FileEdit {
        path: String,
        description: String,
        new_content: String,
    },
    RunCommand {
        command: String,
        working_dir: Option<String>,
        description: String,
    },
}
