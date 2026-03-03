# Homebrew cask management via nix-darwin
#
# This module manages Homebrew packages declaratively through nix-darwin.
# It handles taps, formulae, and casks with separate options for shared
# and personal-only packages.
{
  config,
  lib,
  pkgs,
  ...
}:
with lib;
let
  cfg = config.modules.homebrew;
in
{
  options.modules.homebrew = {
    enable = mkEnableOption "Homebrew management via nix-darwin";

    # Cask categories
    casks = {
      shared = mkOption {
        type = types.listOf types.str;
        default = [
          # Password management
          "1password"
          "1password-cli"
          "bitwarden"

          # Terminal & Development
          "ghostty"
          "dbeaver-community"
          "visual-studio-code"

          # Window management
          "aerospace"
          "claude-code"

          # Productivity
          "raycast"
          "obsidian"

          # Media
          "jellyfin-media-player"
          "ollama-app"

          # Networking & IoT
          "localsend"
          "home-assistant"

          "cursor-cli"
          "claude-code"
          "claude"
        ];
        description = "Homebrew casks to install on all machines";
      };

      personal = mkOption {
        type = types.listOf types.str;
        default = [
          # Photography
          "darktable"

          # Privacy & Security (Proton suite)
          "proton-mail-bridge"
          "proton-pass"
          "protonvpn"

          # Communication
          "signal"
          "thunderbird"

          # Gaming
          "steam"

          # Web
          "zen"
        ];
        description = "Homebrew casks to install only on personal machines";
      };

      enablePersonal = mkOption {
        type = types.bool;
        default = false;
        description = "Whether to install personal-only casks";
      };

      work = mkOption {
        type = types.listOf types.str;
        default = [
          # Communication
          "slack"
          "pritunl"
          "google-chrome"
          "cursor"
        ];
        description = "Homebrew casks to install only on work machines";
      };

      enableWork = mkOption {
        type = types.bool;
        default = false;
        description = "Whether to install work-only casks";
      };
    };

    # Homebrew formulae (for packages not available or preferred from Homebrew)
    brews = mkOption {
      type = types.listOf types.str;
      default = [
        # These are from custom taps or preferred from Homebrew
        "coder/coder/coder"
        "fluxcd/tap/flux"
        "sst/tap/opencode"
        "tree-sitter-cli"
        "mpv"
        "trivy"
      ];
      description = "Homebrew formulae to install (for packages not in nixpkgs)";
    };

    # Required taps
    taps = mkOption {
      type = types.listOf types.str;
      default = [
        "coder/coder"
        "felixkratz/formulae"
        "fluxcd/tap"
        "nikitabobko/tap"
        "sst/tap"
      ];
      description = "Homebrew taps to add";
    };

    # Cleanup behavior
    cleanup = mkOption {
      type = types.enum [
        "none"
        "uninstall"
        "zap"
      ];
      default = "zap";
      description = ''
        Cleanup behavior for Homebrew packages:
        - none: Don't remove anything
        - uninstall: Remove packages not in the configuration
        - zap: Remove packages and their associated files (most aggressive)
      '';
    };
  };

  config = mkIf cfg.enable {
    # Enable Homebrew support in nix-darwin
    homebrew = {
      enable = true;

      # Activation settings
      onActivation = {
        # Auto-update Homebrew itself
        autoUpdate = true;
        # Upgrade outdated packages
        upgrade = true;
        # Cleanup behavior for unmanaged packages
        inherit (cfg) cleanup;
      };

      # Global settings
      global = {
        # Don't auto-update before every brew command
        autoUpdate = false;
        # Use Brewfile lockfile
        brewfile = true;
      };

      # Taps (third-party repositories)
      inherit (cfg) taps;

      # Formulae (CLI tools from Homebrew)
      inherit (cfg) brews;

      caskArgs.no_quarantine = true;

      # Casks (GUI applications)
      casks =
        cfg.casks.shared
        ++ (if cfg.casks.enablePersonal then cfg.casks.personal else [ ])
        ++ (if cfg.casks.enableWork then cfg.casks.work else [ ]);
    };
  };
}
