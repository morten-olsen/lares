use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub api: ApiConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub profile: ProfileConfig,
    #[serde(default)]
    pub build: BuildConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub key: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    #[serde(default = "default_config_repo")]
    pub config_repo: String,
    #[serde(default)]
    pub socket: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProfileConfig {
    /// Active profile name (e.g. "personal", "work").
    /// Determines which darwinConfigurations/nixosConfigurations to build.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BuildConfig {
    /// Custom dry-run/test command (e.g. "make check")
    pub test_command: Option<String>,
    /// Custom rebuild/apply command (e.g. "make switch")
    pub apply_command: Option<String>,
}

fn default_model() -> String {
    "anthropic/claude-sonnet-4.5".into()
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_base_url() -> String {
    "https://openrouter.ai/api/v1".into()
}

/// Platform-specific default config repo path.
/// macOS: /Library/Lares
/// Linux: /etc/lares
pub fn default_config_repo() -> String {
    platform_config_repo().display().to_string()
}

fn platform_config_repo() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Lares")
    } else {
        PathBuf::from("/etc/lares")
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            config_repo: default_config_repo(),
            socket: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if path.exists() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading config from {}", path.display()))?;
            let mut cfg: Config =
                toml::from_str(&text).with_context(|| "parsing lares.toml")?;
            // env var overrides file key
            if let Ok(env_key) = std::env::var("OPENROUTER_API_KEY") {
                cfg.api.key = Some(env_key);
            }
            Ok(cfg)
        } else {
            // No config file — require env var
            let key = std::env::var("OPENROUTER_API_KEY").ok();
            Ok(Config {
                api: ApiConfig {
                    key,
                    model: default_model(),
                    max_tokens: default_max_tokens(),
                    base_url: default_base_url(),
                },
                paths: PathsConfig::default(),
                profile: ProfileConfig::default(),
                build: BuildConfig::default(),
            })
        }
    }

    pub fn api_key(&self) -> Result<&str> {
        self.api
            .key
            .as_deref()
            .filter(|k| !k.is_empty())
            .context("API key not set. Set OPENROUTER_API_KEY or add key to lares.toml")
    }

    pub fn config_repo(&self) -> PathBuf {
        expand_tilde(&self.paths.config_repo)
    }

    pub fn profile_name(&self) -> Option<&str> {
        self.profile.name.as_deref()
    }

    pub fn test_command(&self) -> Option<&str> {
        self.build.test_command.as_deref()
    }

    pub fn apply_command(&self) -> Option<&str> {
        self.build.apply_command.as_deref()
    }

    pub fn socket_path(&self) -> PathBuf {
        if let Some(ref s) = self.paths.socket {
            PathBuf::from(s)
        } else {
            default_socket_path()
        }
    }
}

/// Resolve the path to lares.toml.
/// Priority: LARES_CONFIG env var > ~/.config/lares.toml > /Library/Lares/lares.toml
fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("LARES_CONFIG") {
        return PathBuf::from(p);
    }
    if let Some(home) = dirs::home_dir() {
        let user_config = home.join(".config/lares.toml");
        if user_config.exists() {
            return user_config;
        }
    }
    platform_config_repo().join("lares.toml")
}

pub fn default_socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/lares-{uid}/lares.sock"))
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    Path::new(p).to_path_buf()
}
