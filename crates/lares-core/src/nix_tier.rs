use anyhow::{bail, Result};
use std::path::Path;

use crate::executor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NixTier {
    NixOS,
    DarwinHomeManager,
    LinuxHomeManager,
}

impl NixTier {
    pub fn label(&self) -> &'static str {
        match self {
            NixTier::NixOS => "NixOS",
            NixTier::DarwinHomeManager => "macOS + nix-darwin + Home Manager",
            NixTier::LinuxHomeManager => "Linux + Nix + Home Manager",
        }
    }

    pub fn rebuild_command(&self) -> &'static str {
        match self {
            NixTier::NixOS => "sudo nixos-rebuild switch --flake .",
            NixTier::DarwinHomeManager => "darwin-rebuild switch --flake .",
            NixTier::LinuxHomeManager => "home-manager switch --flake .",
        }
    }

    pub fn rebuild_command_for_profile(&self, profile: Option<&str>) -> String {
        match profile {
            Some(p) => {
                let base = match self {
                    NixTier::NixOS => format!("sudo nixos-rebuild switch --flake .#{p}"),
                    NixTier::DarwinHomeManager => format!("darwin-rebuild switch --flake .#{p}"),
                    NixTier::LinuxHomeManager => format!("home-manager switch --flake .#{p}"),
                };
                base
            }
            None => self.rebuild_command().to_string(),
        }
    }

    /// Returns the nix build command for a dry-run validation.
    /// `profile` is the darwinConfigurations/nixosConfigurations name.
    /// `username` is the homeConfigurations name (HM-only tier).
    pub fn dry_run_command(&self, profile: Option<&str>, username: &str) -> String {
        let flags = "--extra-experimental-features 'nix-command flakes'";
        match self {
            NixTier::DarwinHomeManager => {
                let name = profile.unwrap_or("default");
                format!("nix {flags} build '.#darwinConfigurations.{name}.system' --dry-run")
            }
            NixTier::NixOS => {
                let name = profile.unwrap_or("default");
                format!("nix {flags} build '.#nixosConfigurations.{name}.config.system.build.toplevel' --dry-run")
            }
            NixTier::LinuxHomeManager => {
                let name = profile.unwrap_or(username);
                format!("nix {flags} build '.#homeConfigurations.{name}.activationPackage' --dry-run")
            }
        }
    }

    pub fn nix_system_triple(&self) -> String {
        let arch = std::env::consts::ARCH;
        let nix_arch = match arch {
            "aarch64" => "aarch64",
            "x86_64" => "x86_64",
            other => other,
        };
        match self {
            NixTier::NixOS | NixTier::LinuxHomeManager => format!("{nix_arch}-linux"),
            NixTier::DarwinHomeManager => format!("{nix_arch}-darwin"),
        }
    }

    pub fn has_system_config(&self) -> bool {
        matches!(self, NixTier::NixOS | NixTier::DarwinHomeManager)
    }
}

/// Detect the nix tier. Prefers repo-based detection (reading flake.nix) if available,
/// falls back to system probing.
pub async fn detect(repo_path: &Path) -> Result<NixTier> {
    let flake_path = repo_path.join("flake.nix");
    if flake_path.exists() {
        if let Ok(tier) = detect_from_repo(&flake_path).await {
            return Ok(tier);
        }
    }
    detect_from_system().await
}

async fn detect_from_repo(flake_path: &Path) -> Result<NixTier> {
    let content = tokio::fs::read_to_string(flake_path).await?;

    if content.contains("nixosConfigurations") {
        return Ok(NixTier::NixOS);
    }
    if content.contains("darwinConfigurations") {
        return Ok(NixTier::DarwinHomeManager);
    }
    if content.contains("homeConfigurations") {
        let is_macos = std::env::consts::OS == "macos";
        return Ok(if is_macos {
            NixTier::DarwinHomeManager
        } else {
            NixTier::LinuxHomeManager
        });
    }

    bail!("flake.nix exists but contains no recognized configuration outputs")
}

async fn detect_from_system() -> Result<NixTier> {
    // NixOS: /etc/NIXOS exists
    if Path::new("/etc/NIXOS").exists() {
        return Ok(NixTier::NixOS);
    }

    let is_macos = std::env::consts::OS == "macos";

    // darwin-rebuild available → DarwinHomeManager
    if let Ok(out) = executor::run_command("which darwin-rebuild 2>/dev/null", None).await {
        if out.success {
            return Ok(NixTier::DarwinHomeManager);
        }
    }

    // home-manager available
    if let Ok(out) = executor::run_command("which home-manager 2>/dev/null", None).await {
        if out.success {
            return Ok(if is_macos {
                NixTier::DarwinHomeManager
            } else {
                NixTier::LinuxHomeManager
            });
        }
    }

    // Check if nix is available at all (user may have nix but not home-manager yet)
    if let Ok(out) = executor::run_command("which nix 2>/dev/null", None).await {
        if out.success {
            return Ok(if is_macos {
                NixTier::DarwinHomeManager
            } else {
                NixTier::LinuxHomeManager
            });
        }
    }

    bail!(
        "Nix is required but was not detected.\n\
         Install Nix: https://nixos.org/download\n\
         Then install Home Manager: https://nix-community.github.io/home-manager/"
    )
}
