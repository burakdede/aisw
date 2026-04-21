---
title: Quickstart
description: Install aisw, store your first profiles, and switch between Claude Code, Codex CLI, and Gemini CLI accounts in under five minutes.
---

# Quickstart

From install to switching accounts in five minutes.

## 1. Install

```sh
# Homebrew (macOS and Linux)
brew tap burakdede/tap
brew install aisw

# Shell installer (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh

# Cargo
cargo install aisw
```

Verify:

```sh
aisw --version
```

## 2. Bootstrap

```sh
aisw init
```

This creates `~/.aisw/`, offers to install the optional shell hook (recommended), and detects any accounts you are already logged into. If you are already signed into Claude Code, Codex, or Gemini, `init` will offer to import those credentials as named profiles so you start without re-authenticating.

## 3. Add profiles

**API key:**

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"
```

**From the current environment variable** (useful in CI or when the key is already exported):

```sh
aisw add codex ci --from-env
```

**Interactive OAuth** (opens your browser):

```sh
aisw add claude personal
aisw add codex personal
aisw add gemini personal
```

**Capture the currently logged-in account** (no re-login):

```sh
aisw add claude work --from-live
```

Useful flags:

| Flag | Effect |
|---|---|
| `--label "..."` | Human-readable description shown in `list` and `status` |
| `--set-active` | Activates the profile immediately after adding |

## 4. Switch accounts

Switch a single tool:

```sh
aisw use claude work
aisw use codex personal
aisw use gemini work
```

Switch all tools to the same profile name in one command:

```sh
aisw use --all --profile work
```

**State mode** (Claude Code and Codex CLI only):

```sh
# Isolated: tool reads from a profile-specific config dir (no shared history)
aisw use claude work --state-mode isolated

# Shared: tool reads from its standard config dir (shared history, settings)
aisw use claude work --state-mode shared
```

The default is `isolated`. Use `shared` when you want the tool to behave as if it was never redirected — useful for quick one-off usage or when you rely on existing settings or CLAUDE.md files.

## 5. Inspect state

```sh
# Human-readable summary per tool: installed, active profile, backend, live-match status
aisw status

# Machine-readable (for scripts)
aisw status --json

# List all stored profiles
aisw list
aisw list claude
aisw list --json
```

## 6. Maintain profiles

```sh
# Rename
aisw rename claude default work

# Remove a profile (a backup is created automatically)
aisw remove codex old --yes

# List backups
aisw backup list

# Restore a backup, then re-activate
aisw backup restore 20260325T114502Z-claude-work --yes
aisw use claude work
```

## 7. Shell hook (optional but recommended)

The shell hook lets `aisw use` apply environment variable exports to the current shell session in addition to writing live config files.

```sh
# Zsh
echo 'eval "$(aisw shell-hook zsh)"' >> ~/.zshrc
source ~/.zshrc

# Bash
echo 'eval "$(aisw shell-hook bash)"' >> ~/.bashrc
source ~/.bashrc

# Fish
echo 'aisw shell-hook fish | source' >> ~/.config/fish/config.fish
```

## Next steps

- [Commands](commands.md) — full syntax for every command
- [Automation and scripting](automation.md) — CI and non-interactive patterns
- [How it works](how-it-works.md) — credential storage, platform details, design decisions
