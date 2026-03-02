use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::{Args, Parser, Subcommand};
use futures::{SinkExt, StreamExt};
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use lares_core::config::{self, Config};
use lares_protocol::{ClientMessage, DaemonEvent, ProposedAction};

#[derive(Parser)]
#[command(name = "lares", about = "AI-native system management")]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    prompt_args: PromptArgs,
}

#[derive(Args)]
struct PromptArgs {
    /// The prompt to send to the daemon
    prompt: Vec<String>,

    /// Resume an existing task by ID
    #[arg(long)]
    task: Option<String>,

    /// Socket path (default: /tmp/lares-{uid}/lares.sock)
    #[arg(long)]
    socket: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new or adopt an existing Nix config repo
    Init(InitArgs),
}

#[derive(Args)]
struct InitArgs {
    /// Adopt an existing local Nix config repo
    #[arg(long, value_name = "PATH")]
    repo: Option<PathBuf>,

    /// Clone a remote repo, then adopt it
    #[arg(long, value_name = "URL", conflicts_with = "repo")]
    clone: Option<String>,

    /// Branch to checkout or create before adopting
    #[arg(long, short)]
    branch: Option<String>,

    /// Profile name for this machine (e.g. "personal", "work")
    #[arg(long, short)]
    profile: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init(args)) => run_init(args).await,
        None => run_prompt(cli.prompt_args).await,
    }
}

async fn run_init(args: InitArgs) -> Result<()> {
    let result = if let Some(url) = args.clone {
        let config = Config::load()?;
        let target = config.config_repo();
        lares_core::init::clone_and_adopt(
            &url, &target, args.branch.as_deref(), args.profile.as_deref(),
        ).await?
    } else if let Some(repo_path) = args.repo {
        lares_core::init::adopt(
            &repo_path, args.branch.as_deref(), args.profile.as_deref(),
        ).await?
    } else {
        if args.branch.is_some() {
            anyhow::bail!("--branch requires --repo or --clone");
        }
        // Fresh scaffold — prompt for API key
        let api_key = prompt_api_key()?;
        let config = Config::load()?;
        let config_repo = config.config_repo();
        lares_core::init::scaffold(&config_repo, &api_key).await?
    };

    println!("{result}");
    Ok(())
}

fn prompt_api_key() -> Result<String> {
    // Check env var first
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        if !key.is_empty() {
            eprintln!("Using API key from OPENROUTER_API_KEY environment variable.");
            return Ok(key);
        }
    }

    eprintln!("Lares needs an API key to communicate with the LLM.");
    eprintln!("Get one at: https://openrouter.ai/keys");
    eprintln!();
    eprint!("API key: ");
    io::stderr().flush()?;

    let mut key = String::new();
    io::stdin().read_line(&mut key)?;
    let key = key.trim().to_string();

    if key.is_empty() {
        anyhow::bail!("API key is required. Set OPENROUTER_API_KEY or enter it when prompted.");
    }

    Ok(key)
}

async fn run_prompt(args: PromptArgs) -> Result<()> {
    let prompt = args.prompt.join(" ");
    if prompt.is_empty() && args.task.is_none() {
        anyhow::bail!("provide a prompt or --task to resume");
    }

    let socket_path = args.socket.unwrap_or_else(|| {
        config::default_socket_path().display().to_string()
    });

    let stream = UnixStream::connect(&socket_path)
        .await
        .with_context(|| format!("connecting to {socket_path} — is laresd running?"))?;

    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

    // Send prompt
    let msg = ClientMessage::Prompt {
        text: prompt,
        task_id: args.task,
    };
    let json = serde_json::to_vec(&msg)?;
    framed.send(Bytes::from(json)).await?;

    // Read events
    while let Some(frame) = framed.next().await {
        let bytes = frame.context("reading frame")?;
        let event: DaemonEvent =
            serde_json::from_slice(&bytes).context("parsing daemon event")?;

        match event {
            DaemonEvent::TaskStarted { task_id } => {
                eprintln!("\x1b[2m[task {task_id}]\x1b[0m");
            }

            DaemonEvent::AgentText { text } => {
                println!("{text}");
            }

            DaemonEvent::ToolExecuting { tool_name, summary } => {
                eprintln!("\x1b[33m  [{tool_name}] {summary}\x1b[0m");
            }

            DaemonEvent::ToolResult {
                tool_name,
                summary,
                success,
            } => {
                let icon = if success { "+" } else { "!" };
                eprintln!("\x1b[2m  [{icon} {tool_name}] {summary}\x1b[0m");
            }

            DaemonEvent::ApprovalRequest { request_id, action } => {
                let approved = prompt_approval(&action)?;
                let resp = ClientMessage::ApprovalResponse {
                    request_id,
                    approved,
                };
                let json = serde_json::to_vec(&resp)?;
                framed.send(Bytes::from(json)).await?;
            }

            DaemonEvent::Question { request_id, text } => {
                eprintln!("\x1b[36m  ? {text}\x1b[0m");
                let mut input = String::new();
                print!("> ");
                io::stdout().flush()?;
                io::stdin().read_line(&mut input)?;
                let resp = ClientMessage::UserReply {
                    request_id,
                    text: input.trim().into(),
                };
                let json = serde_json::to_vec(&resp)?;
                framed.send(Bytes::from(json)).await?;
            }

            DaemonEvent::TaskCompleted { task_id, summary } => {
                eprintln!("\x1b[32m[done: {task_id}] {summary}\x1b[0m");
                break;
            }

            DaemonEvent::TaskFailed { task_id, error } => {
                eprintln!("\x1b[31m[failed: {task_id}] {error}\x1b[0m");
                break;
            }

            DaemonEvent::Error { message } => {
                eprintln!("\x1b[31m[error] {message}\x1b[0m");
            }
        }
    }

    Ok(())
}

fn prompt_approval(action: &ProposedAction) -> Result<bool> {
    match action {
        ProposedAction::FileEdit {
            path,
            description,
            new_content,
        } => {
            eprintln!();
            eprintln!("\x1b[1;33mApproval required: edit file\x1b[0m");
            eprintln!("  Path: {path}");
            eprintln!("  Description: {description}");
            let preview: Vec<&str> = new_content.lines().take(20).collect();
            eprintln!("  Content preview:");
            for line in &preview {
                eprintln!("    {line}");
            }
            if new_content.lines().count() > 20 {
                eprintln!("    ... ({} more lines)", new_content.lines().count() - 20);
            }
        }
        ProposedAction::RunCommand {
            command,
            working_dir,
            description,
        } => {
            eprintln!();
            eprintln!("\x1b[1;33mApproval required: run command\x1b[0m");
            eprintln!("  Command: {command}");
            if let Some(dir) = working_dir {
                eprintln!("  Directory: {dir}");
            }
            eprintln!("  Description: {description}");
        }
    }

    eprint!("\x1b[1mApprove? [y/N] \x1b[0m");
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}
