use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{oneshot, Mutex};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{error, info};
use uuid::Uuid;

use lares_core::agent::{AgentEvent, AgentLoop};
use lares_core::config::Config;
use lares_core::task::TaskStore;
use lares_protocol::{ClientMessage, DaemonEvent, ProposedAction};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::load().context("loading config")?;
    let socket_path = config.socket_path();

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding to {}", socket_path.display()))?;
    info!("laresd listening on {}", socket_path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let config = config.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_session(stream, config).await {
                        error!("session error: {e:#}");
                    }
                });
            }
            Err(e) => error!("accept error: {e}"),
        }
    }
}

type PendingApprovals = Arc<Mutex<HashMap<Uuid, oneshot::Sender<bool>>>>;
type PendingReplies = Arc<Mutex<HashMap<Uuid, oneshot::Sender<String>>>>;

async fn handle_session(stream: UnixStream, config: Config) -> Result<()> {
    // Identify the connecting user. In production, this would use SO_PEERCRED
    // for kernel-verified identity. For now, use $USER.
    let username = std::env::var("USER").unwrap_or_else(|_| "unknown".into());

    let framed = Framed::new(stream, LengthDelimitedCodec::new());
    let (sink, mut source) = framed.split();
    let sink = Arc::new(Mutex::new(sink));

    let pending: PendingApprovals = Arc::new(Mutex::new(HashMap::new()));
    let pending_replies: PendingReplies = Arc::new(Mutex::new(HashMap::new()));

    while let Some(frame) = source.next().await {
        let bytes = frame.context("reading frame")?;
        let msg: ClientMessage =
            serde_json::from_slice(&bytes).context("parsing client message")?;

        match msg {
            ClientMessage::Prompt { text, task_id } => {
                let config = config.clone();
                let sink = Arc::clone(&sink);
                let pending = Arc::clone(&pending);
                let pending_replies = Arc::clone(&pending_replies);
                let username = username.clone();

                tokio::spawn(async move {
                    if let Err(e) = run_agent_task(&config, &username, &text, task_id, sink, pending, pending_replies).await {
                        error!("agent task failed: {e:#}");
                    }
                });
            }

            ClientMessage::ApprovalResponse {
                request_id,
                approved,
            } => {
                let mut map = pending.lock().await;
                if let Some(tx) = map.remove(&request_id) {
                    let _ = tx.send(approved);
                }
            }

            ClientMessage::UserReply { request_id, text } => {
                let mut map = pending_replies.lock().await;
                if let Some(tx) = map.remove(&request_id) {
                    let _ = tx.send(text);
                }
            }

            ClientMessage::Cancel => {
                info!("cancel received (not yet implemented)");
            }
        }
    }

    Ok(())
}

type FramedSink = futures::stream::SplitSink<
    Framed<UnixStream, LengthDelimitedCodec>,
    Bytes,
>;

async fn send_event(sink: &Arc<Mutex<FramedSink>>, event: DaemonEvent) -> Result<()> {
    let json = serde_json::to_vec(&event)?;
    let mut sink = sink.lock().await;
    sink.send(Bytes::from(json)).await?;
    Ok(())
}

async fn run_agent_task(
    config: &Config,
    username: &str,
    prompt: &str,
    task_id: Option<String>,
    sink: Arc<Mutex<FramedSink>>,
    pending: PendingApprovals,
    pending_replies: PendingReplies,
) -> Result<()> {
    let task_store = TaskStore::new(&config.config_repo(), username);

    // Create or resume task
    let mut task = if let Some(ref id) = task_id {
        task_store.load(id)?
    } else {
        task_store.create(prompt, prompt)?
    };

    let tid = task.id.clone();
    send_event(&sink, DaemonEvent::TaskStarted { task_id: tid.clone() }).await?;

    // Build event sink that forwards to CLI
    let event_sink = {
        let sink = Arc::clone(&sink);
        Arc::new(move |event: AgentEvent| {
            let sink = Arc::clone(&sink);
            let daemon_event = match event {
                AgentEvent::Text(text) => DaemonEvent::AgentText { text },
                AgentEvent::ToolExecuting { tool_name, summary } => {
                    DaemonEvent::ToolExecuting { tool_name, summary }
                }
                AgentEvent::ToolResult {
                    tool_name,
                    summary,
                    success,
                } => DaemonEvent::ToolResult {
                    tool_name,
                    summary,
                    success,
                },
                AgentEvent::TaskCompleted { summary } => DaemonEvent::TaskCompleted {
                    task_id: String::new(), // filled below
                    summary,
                },
            };
            // Fire and forget — we're in a sync callback
            tokio::spawn(async move {
                let _ = send_event(&sink, daemon_event).await;
            });
        })
    };

    // Build approval gate that sends requests to CLI
    let approval_gate: Arc<dyn lares_core::approval::ApprovalGate> = Arc::new(SocketApprovalGate {
        sink: Arc::clone(&sink),
        pending: Arc::clone(&pending),
    });

    let question_gate: Arc<dyn lares_core::approval::QuestionGate> = Arc::new(SocketQuestionGate {
        sink: Arc::clone(&sink),
        pending_replies: Arc::clone(&pending_replies),
    });

    let agent = AgentLoop::new(config.clone(), username.to_string(), task_store.clone(), approval_gate, question_gate, event_sink)?;

    match agent.run(prompt, &mut task).await {
        Ok(()) => {
            // Save task
            let _ = task_store.save(&task);
            send_event(
                &sink,
                DaemonEvent::TaskCompleted {
                    task_id: tid,
                    summary: "Task finished".into(),
                },
            )
            .await?;
        }
        Err(e) => {
            task.add_journal("error", &format!("{e:#}"));
            task.status = lares_core::task::TaskStatus::Failed;
            let _ = task_store.save(&task);
            send_event(
                &sink,
                DaemonEvent::TaskFailed {
                    task_id: tid,
                    error: format!("{e:#}"),
                },
            )
            .await?;
        }
    }

    Ok(())
}

struct SocketApprovalGate {
    sink: Arc<Mutex<FramedSink>>,
    pending: PendingApprovals,
}

#[async_trait::async_trait]
impl lares_core::approval::ApprovalGate for SocketApprovalGate {
    async fn request_approval(&self, action: &ProposedAction) -> Result<bool> {
        let request_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending.lock().await;
            map.insert(request_id, tx);
        }

        send_event(
            &self.sink,
            DaemonEvent::ApprovalRequest {
                request_id,
                action: action.clone(),
            },
        )
        .await?;

        // Block until CLI responds
        let approved = rx.await.unwrap_or(false);
        Ok(approved)
    }
}

struct SocketQuestionGate {
    sink: Arc<Mutex<FramedSink>>,
    pending_replies: PendingReplies,
}

#[async_trait::async_trait]
impl lares_core::approval::QuestionGate for SocketQuestionGate {
    async fn ask_user(&self, question: &str) -> Result<String> {
        let request_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending_replies.lock().await;
            map.insert(request_id, tx);
        }

        send_event(
            &self.sink,
            DaemonEvent::Question {
                request_id,
                text: question.into(),
            },
        )
        .await?;

        let reply = rx.await.unwrap_or_default();
        Ok(reply)
    }
}
