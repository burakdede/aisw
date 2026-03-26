# aisw

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-mark-dark.svg">
    <img src="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-mark-light.svg" alt="aisw logo" width="160" />
  </picture>
</p>

Manage multiple accounts for Claude Code, Codex CLI, and Gemini CLI.

## The problem

AI coding CLI tools have daily and weekly usage quotas. When a quota runs out, work stops. There is no unified tool for switching between accounts across Claude Code, Codex CLI, and Gemini CLI.

## Terminal demo

See the main workflows before installing:

- Embedded player on the website homepage: <https://burakdede.github.io/aisw/>
- Raw Asciinema cast: <https://burakdede.github.io/aisw/demos/aisw-important-workflows.cast>

The demo covers `aisw init`, importing an existing login, adding profiles, switching with `aisw use`, checking `aisw status`, and viewing automatic backups.

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

It detects installed tools, installs the shell hook, and offers to import your existing credentials with sensible defaults: profile name `default` and label `imported`. Imported live credentials are marked active by default when no aisw-managed active profile already exists for that tool.
When `init` marks an imported profile active, it also applies that profile to the live tool config right away.

## Command reference

| Command | Description |
|---|---|
| `aisw add <tool> <name>` | Add a new account profile |
| `aisw use <tool> <name>` | Switch to a profile |
| `aisw list [tool]` | List all stored profiles |
| `aisw rename <tool> <old> <new>` | Rename a stored profile |
| `aisw remove <tool> <name>` | Delete a profile |
| `aisw status` | Show active profiles and credential health |
| `aisw backup list` | List credential backups |
| `aisw backup restore <backup_id>` | Restore credentials from a backup |
| `aisw init` | First-run setup wizard |
| `aisw shell-hook <shell>` | Print the shell integration snippet |

See the command reference at <https://burakdede.github.io/aisw/commands/> for full flag reference and examples.

Successful `init`, `add`, `use`, and `backup restore` commands also print a short next-step hint to help move through the common workflow without adding noise to inventory or status commands.

## Automation and prompts

`aisw` does not currently have a global `--non-interactive` or `--quiet` flag.

Current automation-safe usage:

- Use `--yes` for commands that otherwise prompt for confirmation:
  - `aisw init --yes`
  - `aisw remove ... --yes`
  - `aisw backup restore ... --yes`
- Use `aisw add ... --api-key ...` when you need non-interactive profile creation. Without `--api-key`, `aisw add` uses an interactive auth flow.
- Use `--json` for machine-readable inventory/status output:
  - `aisw list --json`
  - `aisw status --json`
  - `aisw backup list --json`
- `aisw use --emit-env` and `aisw shell-hook` intentionally print raw shell output for scripting and shell integration.

If you need fully non-interactive automation today, prefer API-key-based `add`, explicit `--yes` flags, and the JSON output modes above.

## How it works

- aisw stores named credential profiles under `~/.aisw/profiles/<tool>/<name>/`.
- `aisw use` applies the selected profile into the live config location each tool actually reads.
- `aisw init` imports detected live credentials as profiles and applies them immediately when it marks them active.
- No proxy, no traffic interception, no network calls. aisw touches only credential files on disk.

## Security

- All credential files are stored with `0600` permissions (owner read/write only).
- `aisw status` checks and reports if any credential file has permissions broader than `0600`.
- aisw never prints credential values to stdout or stderr.
- No credentials are sent over the network by aisw itself.

## Shell integration

`aisw use` now applies the selected profile directly to the live tool config locations, so normal switching does not depend on shell hooks. The shell hook remains available as an optional integration for manual or advanced shell workflows.

Add to your shell config:

```sh
# Bash (~/.bashrc) or Zsh (~/.zshrc)
eval "$(aisw shell-hook bash)"

# Fish (~/.config/fish/config.fish)
aisw shell-hook fish | source
```

Or run `aisw init` — it adds the line automatically.

See <https://burakdede.github.io/aisw/shell-integration/> for details.

## Supported tools

| Tool | Binary | Auth methods |
|---|---|---|
| Claude Code | `claude` | OAuth (browser), API key |
| Codex CLI | `codex` | OAuth (ChatGPT), API key |
| Gemini CLI | `gemini` | OAuth (Google), API key |

## License

MIT.
