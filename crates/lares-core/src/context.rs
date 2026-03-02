use anyhow::Result;
use std::path::Path;

use crate::executor;
use crate::nix_tier::{self, NixTier};
use crate::task::TaskStore;

pub async fn build_system_prompt(
    config_repo: &Path,
    username: &str,
    task_store: &TaskStore,
) -> Result<String> {
    let tier = nix_tier::detect(config_repo).await?;
    let info = gather_system_info().await;
    let nix = gather_nix_info(tier).await;
    let repo_map = build_repo_map(config_repo).await;
    let git_log = recent_git_log(config_repo).await;
    let tasks = summarize_tasks(task_store);
    let where_to_put = where_to_put_changes(tier, username);

    Ok(format!(
        r#"You are Lares, an AI system management agent.
Platform: {tier_label}
System triple: {triple}
User: {username}

## About this platform

{tier_description}

## Config repo: {repo}

{repo_map}

## Where to put changes

{where_to_put}

## System information

{info}

## Nix environment

{nix}

## Recent git history

{git_log}

## Active tasks

{tasks}

## Tools

You have access to: read_file, edit_file, run_command, ask_user, apply_changes, complete_task.
- Use read_file to inspect configuration before making changes.
- Use edit_file to modify nix configuration files. Always provide the full file content.
- Use run_command for read-only informational commands only (nix --version, uname, cat, etc).
- Use ask_user when you need clarification from the user.
- Use apply_changes with a descriptive commit message when your edits are ready. It validates with a dry-run build, commits, and rebuilds atomically. If the dry-run fails, it returns the error so you can fix and retry.
- Use complete_task when you've finished the user's request.

## Workflow

1. Understand what the user wants
2. Read relevant nix config files
3. Edit the config files
4. Call apply_changes with a descriptive commit message
5. If it fails: read the error, fix with more edits, retry apply_changes
6. Complete the task with a summary

## Safety rules

- Never overwrite the repo structure (don't delete flake.nix, system/, users/, lares/)
- Always read a file before editing it
- Do NOT run git commands or rebuild commands directly — use apply_changes
- New automations go in lares/automations/{username}/<name>.nix — never modify existing files without reading first
- One logical change per apply_changes call with a descriptive message
- Only modify files under users/{username}/ and lares/automations/{username}/ — system/ requires elevated trust"#,
        tier_label = tier.label(),
        triple = tier.nix_system_triple(),
        tier_description = tier_description(tier, username),
        repo = config_repo.display(),
    ))
}

fn tier_description(tier: NixTier, username: &str) -> String {
    match tier {
        NixTier::NixOS => format!("\
NixOS provides full declarative system management. The entire system — kernel, \
services, packages, users — is defined in Nix configuration. Changes to \
system/default.nix affect the OS-level config (services, boot, networking). \
Changes to users/{username}/default.nix and its imports affect the user environment \
(dotfiles, user packages, shell config). Both are applied atomically via \
`nixos-rebuild switch`."),
        NixTier::DarwinHomeManager => format!("\
macOS with nix-darwin + Home Manager. nix-darwin manages system-level settings \
(macOS defaults, launch daemons, environment) via system/default.nix. Home Manager \
manages user-level config (packages, dotfiles, shell) via users/{username}/default.nix \
and its imports. Not everything on macOS is declarative — App Store apps, some system \
preferences, and GUI app configs may need imperative changes. The `darwin-rebuild \
switch` command applies both system and home changes atomically."),
        NixTier::LinuxHomeManager => format!("\
Linux with Nix + Home Manager (non-NixOS). Home Manager manages the user \
environment — packages, dotfiles, shell configuration — but does not control \
the system level (no services, kernel, or system packages). System-level changes \
must be done through the host distro's package manager. Changes are applied via \
`home-manager switch`."),
    }
}

fn where_to_put_changes(tier: NixTier, username: &str) -> String {
    let mut lines = vec![
        "| Intent | File | Notes |".to_string(),
        "|--------|------|-------|".to_string(),
    ];

    if tier.has_system_config() {
        match tier {
            NixTier::NixOS => {
                lines.push("| System packages | `system/default.nix` | `environment.systemPackages` |".into());
                lines.push("| Services | `system/default.nix` | `services.*` |".into());
                lines.push("| Networking | `system/default.nix` | `networking.*` |".into());
            }
            NixTier::DarwinHomeManager => {
                lines.push("| System defaults | `system/default.nix` | `system.defaults.*` |".into());
                lines.push("| System packages | `system/default.nix` | `environment.systemPackages` |".into());
                lines.push("| Launch daemons | `system/default.nix` | `launchd.daemons.*` |".into());
            }
            _ => {}
        }
    }

    lines.push(format!("| User packages | `users/{username}/packages.nix` | `home.packages` |"));
    lines.push(format!("| Shell config | `users/{username}/shell.nix` | `programs.zsh.*`, `programs.bash.*`, etc |"));
    lines.push(format!("| Dotfiles | `users/{username}/default.nix` | `home.file.*` or `xdg.configFile.*` |"));
    lines.push(format!("| New automation | `lares/automations/{username}/<name>.nix` | Auto-imported by user config |"));
    lines.push("| Shared automation | `lares/automations/shared/<name>.nix` | Auto-imported for all users |".into());

    lines.join("\n")
}

async fn gather_system_info() -> String {
    let mut lines = vec![];

    lines.push(format!("- Architecture: {}", std::env::consts::ARCH));
    lines.push(format!("- OS: {}", std::env::consts::OS));

    if let Ok(out) = executor::run_command("sw_vers 2>/dev/null", None).await {
        if out.success {
            for line in out.stdout.lines() {
                lines.push(format!("- {}", line.trim()));
            }
        }
    }

    if let Ok(hostname) = hostname::get() {
        lines.push(format!("- Hostname: {}", hostname.to_string_lossy()));
    }

    lines.join("\n")
}

async fn gather_nix_info(tier: NixTier) -> String {
    let mut lines = vec![];

    lines.push(format!("- Tier: {}", tier.label()));

    if let Ok(out) = executor::run_command("nix --version 2>/dev/null", None).await {
        if out.success {
            lines.push(format!("- {}", out.stdout.trim()));
        }
    }

    match tier {
        NixTier::NixOS => {
            if let Ok(out) = executor::run_command(
                "nixos-rebuild list-generations 2>/dev/null | tail -1",
                None,
            ).await {
                if out.success && !out.stdout.trim().is_empty() {
                    lines.push(format!("- Current generation: {}", out.stdout.trim()));
                }
            }
        }
        NixTier::DarwinHomeManager => {
            if let Ok(out) = executor::run_command("which darwin-rebuild 2>/dev/null", None).await {
                if out.success {
                    lines.push("- nix-darwin: available".into());
                }
            }
            if let Ok(out) = executor::run_command(
                "darwin-rebuild --list-generations 2>/dev/null | tail -1",
                None,
            ).await {
                if out.success && !out.stdout.trim().is_empty() {
                    lines.push(format!("- Current generation: {}", out.stdout.trim()));
                }
            }
        }
        NixTier::LinuxHomeManager => {
            if let Ok(out) = executor::run_command(
                "home-manager generations 2>/dev/null | head -1",
                None,
            ).await {
                if out.success && !out.stdout.trim().is_empty() {
                    lines.push(format!("- Current generation: {}", out.stdout.trim()));
                }
            }
        }
    }

    if let Ok(out) = executor::run_command("which home-manager 2>/dev/null", None).await {
        if out.success {
            lines.push("- home-manager: available".into());
        }
    }

    if lines.len() <= 1 {
        "Nix not detected".into()
    } else {
        lines.join("\n")
    }
}

async fn build_repo_map(config_repo: &Path) -> String {
    let cmd = format!(
        "find {} -maxdepth 3 -not -path '*/.git/*' -not -path '*/result*' -not -name '.git' | sort",
        config_repo.display()
    );
    let entries = if let Ok(out) = executor::run_command(&cmd, None).await {
        if out.success {
            out.stdout.trim().to_string()
        } else {
            return "(could not list repo)".into();
        }
    } else {
        return "(could not list repo)".into();
    };

    let repo_prefix = format!("{}/", config_repo.display());
    let mut lines = vec!["```".to_string()];

    for entry in entries.lines() {
        let relative = entry.strip_prefix(&repo_prefix).unwrap_or(entry);
        if relative.is_empty() || relative == config_repo.display().to_string() {
            continue;
        }
        let annotation = annotate_path(relative);
        if let Some(note) = annotation {
            lines.push(format!("{relative:<40} -- {note}"));
        } else {
            lines.push(relative.to_string());
        }
    }

    lines.push("```".to_string());
    lines.join("\n")
}

fn annotate_path(relative: &str) -> Option<&'static str> {
    match relative {
        "flake.nix" => Some("Nix flake entry point"),
        "flake.lock" => Some("Pinned dependency versions"),
        "system/default.nix" => Some("System-level configuration"),
        ".gitignore" => Some("Git ignore rules"),
        _ if relative.starts_with("users/") && relative.ends_with("/default.nix") => {
            Some("Home Manager entry point")
        }
        _ if relative.starts_with("users/") && relative.ends_with("/packages.nix") => {
            Some("User packages (home.packages)")
        }
        _ if relative.starts_with("users/") && relative.ends_with("/shell.nix") => {
            Some("Shell configuration")
        }
        _ if relative.starts_with("lares/automations/") && relative.ends_with("/default.nix") => {
            Some("Auto-imports automation modules")
        }
        _ if relative.starts_with("lares/tasks/") && !relative.contains('.') => {
            Some("Task journal directory")
        }
        "lares/state.json" => Some("Task ID counter"),
        _ => None,
    }
}

async fn recent_git_log(config_repo: &Path) -> String {
    let cmd = format!(
        "git -C {} log --oneline -10 2>/dev/null",
        config_repo.display()
    );
    if let Ok(out) = executor::run_command(&cmd, None).await {
        if out.success && !out.stdout.trim().is_empty() {
            return out.stdout.trim().to_string();
        }
    }
    "(no git history)".into()
}

fn summarize_tasks(task_store: &TaskStore) -> String {
    match task_store.list() {
        Ok(tasks) => {
            let open: Vec<_> = tasks
                .iter()
                .filter(|t| t.status == crate::task::TaskStatus::Open)
                .collect();
            if open.is_empty() {
                "No active tasks.".into()
            } else {
                open.iter()
                    .map(|t| format!("- [{}] {}", t.id, t.goal))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        Err(_) => "No active tasks.".into(),
    }
}
