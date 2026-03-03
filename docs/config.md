# Configuration

The Lares daemon reads its configuration from `lares.toml`.

## Resolution Order

The configuration file is searched in the following order:

1. `LARES_CONFIG` environment variable.
2. `~/.config/lares.toml`
3. Platform default:
    - macOS: `/Library/Lares/lares.toml`
    - Linux: `/etc/lares/lares.toml`

## Configuration Options

### `[api]`
Settings for the LLM API provider (OpenRouter).

| Key | Description | Default |
|-----|-------------|---------|
| `key` | OpenRouter API key. Can also be set via `OPENROUTER_API_KEY` env var. | `None` |
| `model` | The LLM model name to use. | `"anthropic/claude-sonnet-4.5"` |
| `max_tokens` | Maximum tokens per request. | `4096` |
| `base_url` | API base URL. | `"https://openrouter.ai/api/v1"` |

### `[paths]`
System and repository paths.

| Key | Description | Default |
|-----|-------------|---------|
| `config_repo` | Path to the Nix configuration repository. Supports `~/`. | `/Library/Lares` or `/etc/lares` |
| `socket` | Path to the Unix domain socket for communication. | `/tmp/lares-{uid}/lares.sock` |

### `[profile]`
Nix profile settings.

| Key | Description | Default |
|-----|-------------|---------|
| `name` | Active profile name (e.g., "personal", "work"). Determines which `darwinConfigurations` or `nixosConfigurations` to build from the flake. | `None` |

### `[build]`
Custom build and apply commands.

| Key | Description | Default |
|-----|-------------|---------|
| `test_command` | Custom dry-run/test command (e.g., `"make check"`). | `None` |
| `apply_command` | Custom rebuild/apply command (e.g., `"make switch"`). | `None` |

## Example `lares.toml`

```toml
[api]
key = "sk-or-v1-..."
model = "anthropic/claude-3.5-sonnet"

[paths]
config_repo = "~/src/nix-config"

[profile]
name = "personal"

[build]
test_command = "nix build .#darwinConfigurations.personal.system --dry-run"
apply_command = "darwin-rebuild switch --flake .#personal"
```
