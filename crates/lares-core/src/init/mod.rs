pub mod templates;

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::executor;
use crate::nix_tier::{self, NixTier};

pub struct InitResult {
    pub tier: NixTier,
    pub path: std::path::PathBuf,
    pub mode: InitMode,
}

pub enum InitMode {
    Scaffolded,
    Adopted,
}

impl std::fmt::Display for InitResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.mode {
            InitMode::Scaffolded => {
                writeln!(f, "Initialized new {} config at {}", self.tier.label(), self.path.display())?;
                writeln!(f)?;
                writeln!(f, "Next steps:")?;
                writeln!(f, "  # Review and edit the generated configuration")?;
                writeln!(f, "  {}", self.tier.rebuild_command())?;
            }
            InitMode::Adopted => {
                writeln!(f, "Adopted existing {} config at {}", self.tier.label(), self.path.display())?;
                writeln!(f)?;
                writeln!(f, "Lares directories added. To enable automations, add to your HM imports:")?;
                writeln!(f, "  ++ (import ../../lares/automations/shared/default.nix)")?;
                writeln!(f, "  ++ (import ../../lares/automations/<username>/default.nix)")?;
            }
        }
        Ok(())
    }
}

/// Scaffold a fresh config repo.
pub async fn scaffold(config_repo: &Path, api_key: &str) -> Result<InitResult> {
    if config_repo.join("flake.nix").exists() {
        bail!(
            "flake.nix already exists at {}. Use --repo to adopt an existing config.",
            config_repo.display()
        );
    }

    let tier = nix_tier::detect(config_repo).await?;
    let hostname = get_hostname();
    let username = get_username()?;
    let home_dir = get_home_dir(&username)?;

    // Create directories
    let user_dir = format!("users/{username}");
    tokio::fs::create_dir_all(config_repo.join(&user_dir)).await?;
    tokio::fs::create_dir_all(config_repo.join(format!("lares/tasks/{username}"))).await?;
    tokio::fs::create_dir_all(config_repo.join("lares/automations/shared")).await?;
    tokio::fs::create_dir_all(config_repo.join(format!("lares/automations/{username}"))).await?;

    if tier.has_system_config() {
        tokio::fs::create_dir_all(config_repo.join("system")).await?;
    }

    // Write config files
    let repo = config_repo.display().to_string();

    write_file(config_repo, "flake.nix", &templates::flake_nix(tier, &hostname, &username)).await?;
    write_file(config_repo, &format!("{user_dir}/default.nix"), &templates::user_default_nix(&username, &home_dir)).await?;
    write_file(config_repo, &format!("{user_dir}/packages.nix"), &templates::user_packages_nix()).await?;
    write_file(config_repo, &format!("{user_dir}/shell.nix"), &templates::user_shell_nix()).await?;
    write_file(config_repo, "lares/automations/shared/default.nix", &templates::automations_default_nix()).await?;
    write_file(config_repo, &format!("lares/automations/{username}/default.nix"), &templates::automations_default_nix()).await?;
    write_file(config_repo, ".gitignore", &templates::nix_gitignore()).await?;

    if tier.has_system_config() {
        write_file(config_repo, "system/default.nix", &templates::system_default_nix(tier)).await?;
    }

    // Write lares.toml (gitignored — contains API key)
    let repo_str = config_repo.display().to_string();
    write_file(config_repo, "lares.toml", &templates::lares_toml(api_key, None, Some(&repo_str))).await?;

    // git init + initial commit
    executor::run_command("git init", Some(&repo)).await
        .context("git init")?;
    executor::run_command("git add -A", Some(&repo)).await
        .context("git add")?;
    executor::run_command(
        "git commit -m 'Initial configuration (scaffolded by lares)'",
        Some(&repo),
    ).await.context("git commit")?;

    Ok(InitResult {
        tier,
        path: config_repo.to_path_buf(),
        mode: InitMode::Scaffolded,
    })
}

/// Checkout or create a branch in an existing repo.
async fn checkout_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let repo = repo_path.display().to_string();

    // Try checking out an existing branch first
    let out = executor::run_command(
        &format!("git checkout {}", shell_escape(branch)),
        Some(&repo),
    )
    .await?;

    if !out.success {
        // Branch doesn't exist locally — create it
        let out = executor::run_command(
            &format!("git checkout -b {}", shell_escape(branch)),
            Some(&repo),
        )
        .await
        .context("git checkout -b")?;

        if !out.success {
            bail!("failed to create branch '{}': {}", branch, out.stderr.trim());
        }
    }

    Ok(())
}

/// Adopt an existing nix config repo — only add lares/ overlay.
pub async fn adopt(repo_path: &Path, branch: Option<&str>, profile: Option<&str>) -> Result<InitResult> {
    if !repo_path.join("flake.nix").exists() {
        bail!(
            "No flake.nix found at {}. This doesn't appear to be a Nix config repo.",
            repo_path.display()
        );
    }

    if let Some(branch) = branch {
        checkout_branch(repo_path, branch).await?;
    }

    let tier = nix_tier::detect(repo_path).await?;
    let username = get_username()?;
    let mut created_anything = false;
    let repo = repo_path.display().to_string();

    // Create lares/tasks/<username>/ if missing
    let tasks_dir = repo_path.join(format!("lares/tasks/{username}"));
    if !tasks_dir.exists() {
        tokio::fs::create_dir_all(&tasks_dir).await?;
        created_anything = true;
    }

    // Create lares/automations/shared/default.nix if missing
    let shared_default = repo_path.join("lares/automations/shared/default.nix");
    if !shared_default.exists() {
        tokio::fs::create_dir_all(shared_default.parent().unwrap()).await?;
        tokio::fs::write(&shared_default, templates::automations_default_nix()).await?;
        created_anything = true;
    }

    // Create lares/automations/<username>/default.nix if missing
    let user_automations_default = repo_path.join(format!("lares/automations/{username}/default.nix"));
    if !user_automations_default.exists() {
        tokio::fs::create_dir_all(user_automations_default.parent().unwrap()).await?;
        tokio::fs::write(&user_automations_default, templates::automations_default_nix()).await?;
        created_anything = true;
    }

    // Write lares.toml (gitignored — machine-local config)
    let lares_toml_path = repo_path.join("lares.toml");
    let repo_abs = tokio::fs::canonicalize(repo_path).await
        .unwrap_or_else(|_| repo_path.to_path_buf());
    let repo_abs_str = repo_abs.display().to_string();

    if !lares_toml_path.exists() {
        let api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
        tokio::fs::write(
            &lares_toml_path,
            templates::lares_toml(&api_key, profile, Some(&repo_abs_str)),
        ).await.context("writing lares.toml")?;
    }

    // Create ~/.config/lares.toml symlink so the daemon can find the config
    if let Some(home) = dirs::home_dir() {
        let user_config = home.join(".config/lares.toml");
        // Remove existing symlink/file if it points elsewhere
        if user_config.exists() || user_config.symlink_metadata().is_ok() {
            let _ = tokio::fs::remove_file(&user_config).await;
        }
        let lares_toml_abs = repo_abs.join("lares.toml");
        tokio::fs::symlink(&lares_toml_abs, &user_config).await
            .context("creating ~/.config/lares.toml symlink")?;
        eprintln!("Linked ~/.config/lares.toml → {}", lares_toml_abs.display());
    }

    if created_anything {
        executor::run_command("git add lares/", Some(&repo)).await
            .context("git add lares/")?;
        executor::run_command(
            "git commit -m 'lares: add task and automation directories'",
            Some(&repo),
        ).await.context("git commit")?;
    }

    // Validate with a dry-run build
    let dry_run_cmd = tier.dry_run_command(profile, &username);
    eprintln!("Validating configuration (dry-run)...");
    let out = executor::run_command(&dry_run_cmd, Some(&repo)).await
        .context("dry-run build")?;
    if !out.success {
        let stderr = out.stderr.trim();
        bail!(
            "Dry-run build failed — the configuration may have errors.\n\
             Command: {dry_run_cmd}\n\
             {stderr}"
        );
    }
    eprintln!("Configuration is valid.");

    Ok(InitResult {
        tier,
        path: repo_path.to_path_buf(),
        mode: InitMode::Adopted,
    })
}

/// Clone a remote repo, then adopt it.
pub async fn clone_and_adopt(url: &str, target: &Path, branch: Option<&str>, profile: Option<&str>) -> Result<InitResult> {
    let target_str = target.display().to_string();
    let out = executor::run_command(
        &format!("git clone {} {}", shell_escape(url), shell_escape(&target_str)),
        None,
    ).await.context("git clone")?;

    if !out.success {
        bail!("git clone failed: {}", out.stderr.trim());
    }

    adopt(target, branch, profile).await
}

async fn write_file(base: &Path, relative: &str, content: &str) -> Result<()> {
    let path = base.join(relative);
    tokio::fs::write(&path, content)
        .await
        .with_context(|| format!("writing {}", path.display()))
}

fn get_hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "nixhost".into())
}

/// Get the target username. Under sudo, uses SUDO_USER to get the original user.
fn get_username() -> Result<String> {
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.is_empty() {
            return Ok(sudo_user);
        }
    }
    std::env::var("USER").context("USER environment variable not set")
}

fn get_home_dir(username: &str) -> Result<String> {
    // Under sudo, dirs::home_dir() returns root's home. Use the username to construct it.
    if std::env::var("SUDO_USER").is_ok() {
        if cfg!(target_os = "macos") {
            return Ok(format!("/Users/{username}"));
        } else {
            return Ok(format!("/home/{username}"));
        }
    }
    dirs::home_dir()
        .map(|p| p.display().to_string())
        .context("could not determine home directory")
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
