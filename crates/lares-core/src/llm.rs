use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;

// ── OpenAI-compatible request types ─────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDef>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

// ── Response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: ResponseMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

// ── Client ──────────────────────────────────────────────────────

pub struct LlmClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl LlmClient {
    pub fn new(config: &Config) -> Result<Self> {
        let api_key = config.api_key()?.to_owned();
        Ok(Self {
            http: reqwest::Client::new(),
            base_url: config.api.base_url.trim_end_matches('/').to_owned(),
            api_key,
            model: config.api.model.clone(),
            max_tokens: config.api.max_tokens,
        })
    }

    pub async fn chat(&self, messages: Vec<Message>, tools: Vec<ToolDef>) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let req = ChatRequest {
            model: self.model.clone(),
            messages,
            tools,
            max_tokens: self.max_tokens,
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req)
            .send()
            .await
            .context("sending request to OpenRouter")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("OpenRouter returned {status}: {body}");
        }

        resp.json::<ChatResponse>()
            .await
            .context("parsing OpenRouter response")
    }
}

// ── Tool definitions ────────────────────────────────────────────

pub fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "read_file".into(),
                description: "Read the contents of a file at the given path.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute or relative path to the file to read"
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "edit_file".into(),
                description: "Write or overwrite a file with the given content. Requires user approval.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute or relative path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "The full new content for the file"
                        },
                        "description": {
                            "type": "string",
                            "description": "Human-readable description of what this edit does"
                        }
                    },
                    "required": ["path", "content", "description"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "run_command".into(),
                description: "Execute a read-only informational shell command (e.g. nix --version, uname, cat). Do NOT use for git, rebuild, or any mutation.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute (read-only only)"
                        },
                        "working_dir": {
                            "type": "string",
                            "description": "Optional working directory for the command"
                        },
                        "description": {
                            "type": "string",
                            "description": "Human-readable description of what this command does"
                        }
                    },
                    "required": ["command", "description"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "ask_user".into(),
                description: "Ask the user a question and wait for their response. Use this when you need clarification, confirmation, or a choice before proceeding.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question to ask the user"
                        }
                    },
                    "required": ["question"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "apply_changes".into(),
                description: "Validate, commit, and rebuild atomically. Stages all changes, runs a dry-run build to validate, commits, then rebuilds. If the dry-run fails, changes are unstaged and an error is returned so you can fix and retry. Call this once after all edits are complete.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Descriptive git commit message for the changes"
                        }
                    },
                    "required": ["message"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "complete_task".into(),
                description: "Mark the current task as completed with a summary.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "summary": {
                            "type": "string",
                            "description": "A summary of what was accomplished"
                        }
                    },
                    "required": ["summary"]
                }),
            },
        },
    ]
}

// ── Helpers ─────────────────────────────────────────────────────

impl Message {
    pub fn system(content: &str) -> Self {
        Self {
            role: "system".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: &str) -> Self {
        Self {
            role: "user".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant_from_response(resp: &ResponseMessage) -> Self {
        Self {
            role: "assistant".into(),
            content: resp.content.clone(),
            tool_calls: resp.tool_calls.clone(),
            tool_call_id: None,
        }
    }

    pub fn tool_result(tool_call_id: &str, content: &str) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}
