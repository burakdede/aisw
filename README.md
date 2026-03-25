# aisw

Manage multiple accounts for Claude Code, Codex CLI, and Gemini CLI.

## The problem

AI coding CLI tools have daily and weekly usage quotas. When a quota runs out, work stops. There is no unified tool for switching between accounts across Claude Code, Codex CLI, and Gemini CLI.

## Install

**curl (Linux, macOS):**

```sh
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh
```

**Homebrew (macOS):**

```sh
brew install burakdede/aisw/aisw
```

**Cargo:**

```sh
cargo install aisw
```

## Quickstart

You hit your Claude Code quota mid-session. You have a second account. Switch in seconds:

```sh
# Add the second account (opens browser for OAuth, or paste an API key with --api-key).
aisw add claude personal

# Switch to it.
aisw use claude personal

# Claude Code now reads credentials from the personal profile.
# Switch back any time.
aisw use claude work
```

First time? Run the setup wizard:

```sh
aisw init
```

It detects installed tools, installs the shell hook, and offers to import your existing credentials as a `default` profile. Imported live credentials are marked active by default when no aisw-managed active profile already exists for that tool.

## Command reference

| Command | Description |
|---|---|
| `aisw add <tool> <name>` | Add a new account profile |
| `aisw use <tool> <name>` | Switch to a profile |
| `aisw list [tool]` | List all stored profiles |
| `aisw remove <tool> <name>` | Delete a profile |
| `aisw status` | Show active profiles and credential health |
| `aisw backup list` | List credential backups |
| `aisw backup restore <backup_id>` | Restore credentials from a backup |
| `aisw init` | First-run setup wizard |
| `aisw shell-hook <shell>` | Print the shell integration snippet |

See [docs/commands.md](docs/commands.md) for full flag reference and examples.

## How it works

- aisw stores named credential profiles under `~/.aisw/profiles/<tool>/<name>/`.
- `aisw use` points each tool at the selected profile by setting environment variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) or rewriting the tool's config file (`~/.gemini/.env`).
- No proxy, no traffic interception, no network calls. aisw touches only credential files on disk.

## Security

- All credential files are stored with `0600` permissions (owner read/write only).
- `aisw status` checks and reports if any credential file has permissions broader than `0600`.
- aisw never prints credential values to stdout or stderr.
- No credentials are sent over the network by aisw itself.

## Shell integration

`aisw use` sets environment variables. For these to take effect in your current shell session, the shell hook must be active. Without it, `aisw` records the selected profile as active but warns that the current shell is not using it yet.

Add to your shell config:

```sh
# Bash (~/.bashrc) or Zsh (~/.zshrc)
eval "$(aisw shell-hook bash)"

# Fish (~/.config/fish/config.fish)
aisw shell-hook fish | source
```

Or run `aisw init` — it adds the line automatically.

See [docs/shell-integration.md](docs/shell-integration.md) for details.

## Supported tools

| Tool | Binary | Auth methods |
|---|---|---|
| Claude Code | `claude` | OAuth (browser), API key (`sk-ant-...`) |
| Codex CLI | `codex` | OAuth (ChatGPT), API key |
| Gemini CLI | `gemini` | OAuth (Google), API key |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT. See [LICENSE](LICENSE).
