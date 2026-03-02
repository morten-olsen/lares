use anyhow::{bail, Result};
use serde_json::Value;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tracing::info;

use lares_protocol::ProposedAction;

use crate::approval::{ApprovalGate, QuestionGate};
use crate::config::Config;
use crate::context;
use crate::executor;
use crate::llm::{self, LlmClient, Message, ToolCall};
use crate::nix_tier;
use crate::task::{Task, TaskStore};

const MAX_ITERATIONS: usize = 50;

/// Events emitted during the agent loop for the caller to forward to the CLI.
#[derive(Debug)]
pub enum AgentEvent {
    Text(String),
    ToolExecuting { tool_name: String, summary: String },
    ToolResult { tool_name: String, summary: String, success: bool },
    TaskCompleted { summary: String },
}

/// Callback for sending events out of the agent loop.
pub type EventSink = Arc<dyn Fn(AgentEvent) -> () + Send + Sync>;

pub struct AgentLoop {
    client: LlmClient,
    config: Config,
    username: String,
    task_store: TaskStore,
    approval_gate: Arc<dyn ApprovalGate>,
    question_gate: Arc<dyn QuestionGate>,
    event_sink: EventSink,
    apply_attempts: AtomicU32,
}

impl AgentLoop {
    pub fn new(
        config: Config,
        username: String,
        task_store: TaskStore,
        approval_gate: Arc<dyn ApprovalGate>,
        question_gate: Arc<dyn QuestionGate>,
        event_sink: EventSink,
    ) -> Result<Self> {
        let client = LlmClient::new(&config)?;
        Ok(Self {
            client,
            config,
            username,
            task_store,
            approval_gate,
            question_gate,
            event_sink,
            apply_attempts: AtomicU32::new(0),
        })
    }

    pub async fn run(&self, prompt: &str, task: &mut Task) -> Result<()> {
        let config_repo = self.config.config_repo();
        let system_prompt = context::build_system_prompt(&config_repo, &self.username, &self.task_store).await?;
        let tools = llm::tool_definitions();

        let mut messages = vec![
            Message::system(&system_prompt),
            Message::user(prompt),
        ];

        for iteration in 0..MAX_ITERATIONS {
            info!(iteration, "agent loop iteration");

            let response = self.client.chat(messages.clone(), tools.clone()).await?;
            let choice = response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("no choices in response"))?;

            let finish_reason = choice.finish_reason.as_deref().unwrap_or("stop");

            // Add assistant message to conversation
            messages.push(Message::assistant_from_response(&choice.message));

            // Emit text content if present
            if let Some(ref text) = choice.message.content {
                if !text.is_empty() {
                    (self.event_sink)(AgentEvent::Text(text.clone()));
                    task.add_journal("observed", text);
                }
            }

            // Process tool calls
            let tool_calls = choice.message.tool_calls.unwrap_or_default();

            if tool_calls.is_empty() {
                // No tool calls — agent is done
                info!("agent finished (no tool calls, finish_reason={finish_reason})");
                break;
            }

            for tc in &tool_calls {
                let result = self.handle_tool_call(tc, task).await;
                let (content, success) = match result {
                    Ok(s) => (s, true),
                    Err(e) => (format!("Error: {e}"), false),
                };

                let summary = truncate(&content, 200);
                (self.event_sink)(AgentEvent::ToolResult {
                    tool_name: tc.function.name.clone(),
                    summary: summary.clone(),
                    success,
                });

                messages.push(Message::tool_result(&tc.id, &content));
            }

            // Check if complete_task was called
            if tool_calls.iter().any(|tc| tc.function.name == "complete_task") {
                info!("agent called complete_task");
                break;
            }
        }

        Ok(())
    }

    async fn handle_tool_call(&self, tc: &ToolCall, task: &mut Task) -> Result<String> {
        let args: Value = serde_json::from_str(&tc.function.arguments)
            .unwrap_or_else(|_| Value::Object(Default::default()));

        match tc.function.name.as_str() {
            "read_file" => {
                let path = args["path"].as_str().unwrap_or("");
                (self.event_sink)(AgentEvent::ToolExecuting {
                    tool_name: "read_file".into(),
                    summary: format!("Reading {path}"),
                });
                task.add_journal("action", &format!("read_file: {path}"));
                let content = executor::read_file(path).await?;
                Ok(content)
            }

            "edit_file" => {
                let path = args["path"].as_str().unwrap_or("");
                let content = args["content"].as_str().unwrap_or("");
                let description = args["description"].as_str().unwrap_or("edit file");

                let action = ProposedAction::FileEdit {
                    path: path.into(),
                    description: description.into(),
                    new_content: content.into(),
                };

                (self.event_sink)(AgentEvent::ToolExecuting {
                    tool_name: "edit_file".into(),
                    summary: format!("Edit {path}: {description}"),
                });

                let approved = self.approval_gate.request_approval(&action).await?;
                if !approved {
                    task.add_journal("result", &format!("User rejected edit to {path}"));
                    return Ok("User rejected this file edit.".into());
                }

                executor::write_file(path, content).await?;
                task.add_journal("action", &format!("edit_file: {path} — {description}"));
                Ok(format!("File written: {path}"))
            }

            "run_command" => {
                let command = args["command"].as_str().unwrap_or("");
                let working_dir = args["working_dir"].as_str();
                let description = args["description"].as_str().unwrap_or("run command");

                (self.event_sink)(AgentEvent::ToolExecuting {
                    tool_name: "run_command".into(),
                    summary: format!("{description}: {command}"),
                });

                task.add_journal("action", &format!("run_command: {command}"));
                let output = executor::run_command(command, working_dir).await?;
                let result = output.to_string();
                task.add_journal("result", &truncate(&result, 500));
                Ok(result)
            }

            "ask_user" => {
                let question = args["question"].as_str().unwrap_or("");
                (self.event_sink)(AgentEvent::ToolExecuting {
                    tool_name: "ask_user".into(),
                    summary: format!("Asking: {}", truncate(question, 100)),
                });
                task.add_journal("action", &format!("ask_user: {question}"));
                let reply = self.question_gate.ask_user(question).await?;
                task.add_journal("result", &format!("User replied: {reply}"));
                Ok(reply)
            }

            "apply_changes" => {
                let message = args["message"].as_str().unwrap_or("apply changes");
                self.handle_apply_changes(message, task).await
            }

            "complete_task" => {
                let summary = args["summary"].as_str().unwrap_or("Task completed");
                self.apply_attempts.store(0, Ordering::Relaxed);
                task.add_journal("resolved", summary);
                task.status = crate::task::TaskStatus::Resolved;
                (self.event_sink)(AgentEvent::TaskCompleted {
                    summary: summary.into(),
                });
                Ok(format!("Task completed: {summary}"))
            }

            other => {
                bail!("unknown tool: {other}")
            }
        }
    }

    async fn handle_apply_changes(&self, message: &str, task: &mut Task) -> Result<String> {
        let config_repo = self.config.config_repo();
        let repo = config_repo.display().to_string();
        let attempt = self.apply_attempts.fetch_add(1, Ordering::Relaxed) + 1;

        (self.event_sink)(AgentEvent::ToolExecuting {
            tool_name: "apply_changes".into(),
            summary: format!("Applying: {message}"),
        });
        task.add_journal("action", &format!("apply_changes: {message}"));

        // 1. Stage everything
        executor::run_command("git add -A", Some(&repo)).await?;

        // 2. Check if anything is staged
        let diff_check = executor::run_command("git diff --cached --quiet", Some(&repo)).await?;
        if diff_check.success {
            return Ok("No changes to apply.".into());
        }

        // 3. Resolve test command
        let tier = nix_tier::detect(&config_repo).await?;
        let profile = self.config.profile_name();
        let test_cmd = match self.config.test_command() {
            Some(cmd) => cmd.to_string(),
            None => tier.dry_run_command(profile, &self.username),
        };

        // 4. Run dry-run
        let dry_run = executor::run_command(&test_cmd, Some(&repo)).await?;

        if !dry_run.success {
            // Dry-run failed — unstage
            let _ = executor::run_command("git reset", Some(&repo)).await;

            let output = dry_run.to_string();
            task.add_journal("result", &format!("dry-run failed (attempt {attempt}): {}", truncate(&output, 500)));

            if attempt >= 3 {
                // Hard failure — discard all changes
                let _ = executor::run_command("git checkout -- .", Some(&repo)).await;
                let _ = executor::run_command("git clean -fd", Some(&repo)).await;
                self.apply_attempts.store(0, Ordering::Relaxed);
                return Ok(format!(
                    "HARD FAILURE after {attempt} attempts. All changes have been discarded.\n\
                     Last build error:\n{output}"
                ));
            }

            return Ok(format!(
                "Dry-run validation failed (attempt {attempt}/3). Fix the errors and call apply_changes again.\n\
                 Build output:\n{output}"
            ));
        }

        // 5. Re-stage (dry-run may have changed lockfiles etc) and commit
        executor::run_command("git add -A", Some(&repo)).await?;
        let commit = executor::run_command(
            &format!("git commit -m {}", shell_escape(message)),
            Some(&repo),
        ).await?;

        if !commit.success {
            let _ = executor::run_command("git reset", Some(&repo)).await;
            return Ok(format!("Git commit failed:\n{}", commit.to_string()));
        }

        // 6. Resolve apply command and rebuild
        let apply_cmd = match self.config.apply_command() {
            Some(cmd) => cmd.to_string(),
            None => tier.rebuild_command_for_profile(profile),
        };

        let rebuild = executor::run_command(&apply_cmd, Some(&repo)).await?;

        if !rebuild.success {
            // Revert the commit
            let _ = executor::run_command("git revert --no-edit HEAD", Some(&repo)).await;
            let output = rebuild.to_string();
            task.add_journal("result", &format!("rebuild failed, reverted: {}", truncate(&output, 500)));
            return Ok(format!(
                "Rebuild failed. The commit has been reverted.\n\
                 Rebuild output:\n{output}"
            ));
        }

        // Success
        self.apply_attempts.store(0, Ordering::Relaxed);
        task.add_journal("result", &format!("apply_changes succeeded: {message}"));
        Ok(format!("Changes applied successfully. Committed and rebuilt: {message}"))
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
