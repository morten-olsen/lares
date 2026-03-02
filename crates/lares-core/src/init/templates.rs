use crate::nix_tier::NixTier;

pub fn flake_nix(tier: NixTier, hostname: &str, username: &str) -> String {
    match tier {
        NixTier::NixOS => format!(
            r#"{{
  description = "NixOS system configuration";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager = {{
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
  }};

  outputs = {{ self, nixpkgs, home-manager, ... }}: {{
    nixosConfigurations.{hostname} = nixpkgs.lib.nixosSystem {{
      modules = [
        ./system/default.nix
        home-manager.nixosModules.home-manager
        {{
          home-manager.useGlobalPkgs = true;
          home-manager.useUserPackages = true;
          home-manager.users.{username} = import ./users/{username}/default.nix;
        }}
      ];
    }};
  }};
}}
"#
        ),
        NixTier::DarwinHomeManager => format!(
            r#"{{
  description = "macOS system configuration";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nix-darwin = {{
      url = "github:LnL7/nix-darwin";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
    home-manager = {{
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
  }};

  outputs = {{ self, nixpkgs, nix-darwin, home-manager, ... }}: {{
    darwinConfigurations.{hostname} = nix-darwin.lib.darwinSystem {{
      modules = [
        ./system/default.nix
        home-manager.darwinModules.home-manager
        {{
          home-manager.useGlobalPkgs = true;
          home-manager.useUserPackages = true;
          home-manager.users.{username} = import ./users/{username}/default.nix;
        }}
      ];
    }};
  }};
}}
"#
        ),
        NixTier::LinuxHomeManager => format!(
            r#"{{
  description = "Home Manager configuration";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    home-manager = {{
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
  }};

  outputs = {{ self, nixpkgs, home-manager, ... }}: {{
    homeConfigurations.{username} = home-manager.lib.homeManagerConfiguration {{
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      modules = [ ./users/{username}/default.nix ];
    }};
  }};
}}
"#
        ),
    }
}

pub fn system_default_nix(tier: NixTier) -> String {
    match tier {
        NixTier::NixOS => r#"{ config, pkgs, ... }:
{
  # NixOS system configuration
  # See: https://nixos.org/manual/nixos/stable/options

  system.stateVersion = "24.11";

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  environment.systemPackages = with pkgs; [
    git
    vim
  ];
}
"#
        .into(),
        NixTier::DarwinHomeManager => r#"{ config, pkgs, ... }:
{
  # nix-darwin system configuration
  # See: https://daiderd.com/nix-darwin/manual/index.html

  system.stateVersion = 5;

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  environment.systemPackages = with pkgs; [
    git
    vim
  ];

  # macOS system defaults
  # system.defaults.dock.autohide = true;
  # system.defaults.finder.AppleShowAllExtensions = true;
}
"#
        .into(),
        NixTier::LinuxHomeManager => String::new(),
    }
}

pub fn user_default_nix(username: &str, home_dir: &str) -> String {
    format!(
        r#"{{ config, pkgs, ... }}:
{{
  # Home Manager entry point
  # See: https://nix-community.github.io/home-manager/options.xhtml

  home.username = "{username}";
  home.homeDirectory = "{home_dir}";
  home.stateVersion = "24.11";

  programs.home-manager.enable = true;

  imports = [
    ./packages.nix
    ./shell.nix
  ] ++ (import ../../lares/automations/shared/default.nix)
    ++ (import ../../lares/automations/{username}/default.nix);
}}
"#
    )
}

pub fn user_packages_nix() -> String {
    r#"{ pkgs, ... }:
{
  # User packages installed via Home Manager
  # Search: https://search.nixos.org/packages

  home.packages = with pkgs; [
    # Development
    # nodejs
    # python3
    # rustup

    # Tools
    ripgrep
    fd
    jq
    tree
  ];
}
"#
    .into()
}

pub fn user_shell_nix() -> String {
    r#"{ pkgs, ... }:
{
  # Shell configuration

  programs.bash.enable = true;

  # programs.zsh.enable = true;
  # programs.zsh.enableCompletion = true;

  # programs.fish.enable = true;

  # Shell aliases
  home.shellAliases = {
    ll = "ls -la";
    gs = "git status";
  };
}
"#
    .into()
}

pub fn automations_default_nix() -> String {
    r#"# Auto-imports all *.nix files in this directory (except default.nix).
# Lares creates automation modules here.
let
  dir = builtins.readDir ./.;
  nixFiles = builtins.filter (n: n != "default.nix" && builtins.match ".*\.nix" n != null)
    (builtins.attrNames dir);
in
  map (f: import (./. + "/${f}")) nixFiles
"#
    .into()
}

pub fn lares_toml(api_key: &str, profile: Option<&str>, config_repo: Option<&str>) -> String {
    let profile_section = match profile {
        Some(name) => format!(
            r#"
[profile]
name = "{name}"
"#
        ),
        None => r#"
# [profile]
# name = "personal"  # Which darwinConfigurations/nixosConfigurations to build
"#
        .into(),
    };

    let paths_section = match config_repo {
        Some(path) => format!(
            r#"
[paths]
config_repo = "{path}"
"#
        ),
        None => r#"
# [paths]
# config_repo = "/Library/Lares"  # Override for development
"#
        .into(),
    };

    format!(
        r#"[api]
key = "{api_key}"
{paths_section}{profile_section}
# [build]
# test_command = "make check"    # Custom dry-run validation command
# apply_command = "make switch"  # Custom rebuild command
"#
    )
}

pub fn nix_gitignore() -> String {
    r#"result
result-*
.direnv
lares.toml
"#
    .into()
}
