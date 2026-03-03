use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
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
    
    // Set socket permissions to allow all users to connect (0666)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&socket_path)?.permissions();
        perms.set_mode(0o666);
        std::fs::set_permissions(&socket_path, perms)?;
    }
    
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
    // Identify the connecting user via SO_PEERCRED (UID from socket)
    let (username, uid, gid) = get_peer_credentials(&stream)?;

    let framed = Framed::new(stream, LengthDelimitedCodec::new());
    let (sink, mut source) = framed.split();
    let sink = Arc::new(Mutex::new(sink));
    let config = Arc::new(config);

    let pending: PendingApprovals = Arc::new(Mutex::new(HashMap::new()));
    let pending_replies: PendingReplies = Arc::new(Mutex::new(HashMap::new()));

    while let Some(frame) = source.next().await {
        let bytes = frame.context("reading frame")?;
        let msg: ClientMessage =
            serde_json::from_slice(&bytes).context("parsing client message")?;

        match msg {
            ClientMessage::Prompt { text, task_id } => {
                let sink = Arc::clone(&sink);
                let config = Arc::clone(&config);
                let pending = Arc::clone(&pending);
                let pending_replies = Arc::clone(&pending_replies);
                let username = username.clone();
                let user_uid = uid;
                let user_gid = gid;
                
                tokio::spawn(async move {
                    if let Err(e) = run_agent_task(&config, &username, user_uid, user_gid, &text, task_id, sink, pending, pending_replies).await {
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
    uid: u32,
    gid: u32,
    text: &str,
    task_id: Option<String>,
    sink: Arc<Mutex<FramedSink>>,
    pending: PendingApprovals,
    pending_replies: PendingReplies,
) -> Result<()> {
    let task_store = TaskStore::with_ownership(&config.config_repo(), username, uid, gid);

    // Create or resume task
    let mut task = if let Some(ref id) = task_id {
        task_store.load(id)?
    } else {
        task_store.create(text, text)?
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
        config_repo: config.config_repo().clone(),
    });

    let question_gate: Arc<dyn lares_core::approval::QuestionGate> = Arc::new(SocketQuestionGate {
        sink: Arc::clone(&sink),
        pending_replies: Arc::clone(&pending_replies),
    });

    let agent = AgentLoop::new(config.clone(), username.to_string(), uid, gid, task_store.clone(), approval_gate, question_gate, event_sink)?;

    match agent.run(text, &mut task).await {
        Ok(_result) => {
            task_store.save(&task)?;
            // Note: TaskCompleted event is already sent by the AgentEvent::TaskCompleted handler above
            // No need to send it again here - it would cause a broken pipe error
        }
        Err(e) => {
            task_store.save(&task)?;
            return Err(e);
        }
    }

    Ok(())
}

struct SocketApprovalGate {
    sink: Arc<Mutex<FramedSink>>,
    pending: PendingApprovals,
    config_repo: PathBuf,
}

#[async_trait::async_trait]
impl lares_core::approval::ApprovalGate for SocketApprovalGate {
    async fn request_approval(&self, action: &ProposedAction) -> Result<bool> {
        // Auto-approve Nix file edits within the config repo
        if let ProposedAction::FileEdit { path, .. } = action {
            let path_buf = PathBuf::from(path);
            
            // Check if file is within config repo and is a .nix file
            if path_buf.starts_with(&self.config_repo) && path_buf.extension().and_then(|s| s.to_str()) == Some("nix") {
                tracing::info!("Auto-approving Nix config edit: {}", path);
                return Ok(true);
            }
        }
        
        // For everything else, request user approval
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

/// Get the UID, GID, and username of the peer connected to this Unix socket
fn get_peer_credentials(stream: &tokio::net::UnixStream) -> Result<(String, u32, u32)> {
    let fd = stream.as_raw_fd();
    
    #[cfg(target_os = "macos")]
    {
        let mut uid: libc::uid_t = 0;
        let mut gid: libc::gid_t = 0;
        
        unsafe {
            if libc::getpeereid(fd, &mut uid, &mut gid) != 0 {
                anyhow::bail!("getpeereid failed");
            }
        }
        
        let username = uid_to_username(uid)?;
        Ok((username, uid, gid))
    }
    
    #[cfg(target_os = "linux")]
    {
        let mut ucred: libc::ucred = unsafe { std::mem::zeroed() };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        
        unsafe {
            if libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                &mut ucred as *mut _ as *mut libc::c_void,
                &mut len,
            ) != 0 {
                anyhow::bail!("getsockopt SO_PEERCRED failed");
            }
        }
        
        let username = uid_to_username(ucred.uid)?;
        Ok((username, ucred.uid, ucred.gid))
    }
}

/// Convert a UID to username using getpwuid
fn uid_to_username(uid: u32) -> Result<String> {
    use std::ffi::CStr;
    
    unsafe {
        let pwd = libc::getpwuid(uid);
        if pwd.is_null() {
            anyhow::bail!("getpwuid failed for uid {}", uid);
        }
        let name_cstr = CStr::from_ptr((*pwd).pw_name);
        Ok(name_cstr.to_string_lossy().into_owned())
    }
}
