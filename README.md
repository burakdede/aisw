# aisw

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-logo.png">
    <img src="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-logo.png" alt="aisw" width="160" />
  </picture>
</p>

<p align="center"><strong>Named profile and context manager for Claude Code, Codex CLI, and Gemini CLI.</strong></p>

<p align="center">Switch between multiple work, personal, and client accounts in one command - across all three AI coding agents.</p>

<p align="center"><em>The answer to "how do I use two Claude Code accounts?" and "how do I switch Codex CLI between client projects?"</em></p>

<p align="center">
  <a href="https://crates.io/crates/aisw">
    <img src="https://img.shields.io/crates/v/aisw?style=flat-square" alt="Crates.io version" />
  </a>
  <a href="https://github.com/burakdede/aisw/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/burakdede/aisw/ci.yml?branch=main&style=flat-square&label=CI" alt="CI status" />
  </a>
  <a href="https://github.com/burakdede/aisw/releases">
    <img src="https://img.shields.io/github/v/release/burakdede/aisw?style=flat-square&label=release" alt="Latest release" />
  </a>
  <a href="https://burakdede.github.io/aisw/">
    <img src="https://img.shields.io/badge/docs-website-4c6fff?style=flat-square" alt="Documentation" />
  </a>
</p>

---

## The problem

Claude Code, Codex CLI, and Gemini CLI each store credentials in different locations - files, OS keychains, and tool-specific directories. If you maintain separate work and personal accounts, or manage credentials for multiple clients, switching means editing hidden files, copying tokens, and hoping nothing breaks.

`aisw` solves this with named profiles and contexts. Profiles capture per-tool auth state. Contexts let you map those profiles across tools into one saved work mode. Switching is a single command that atomically replaces the live credentials and produces a clean rollback on failure.

## Demo

General workflow:

![aisw CLI demo](website/public/demos/aisw-important-workflows.gif)

Context workflow:

![aisw context demo](website/public/demos/aisw-context-workflow.gif)

## Install

```sh
# Homebrew (macOS and Linux)
brew tap burakdede/tap
brew install aisw

# Shell installer (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh

# Cargo
cargo install aisw
```

## Getting started

```sh
# Bootstrap: creates ~/.aisw/, offers shell-hook setup,
# imports any currently logged-in accounts
aisw init

# Store profiles
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"

# Save a cross-tool context when profile names differ
aisw context create acme \
  --claude acme-claude \
  --codex acme-codex \
  --gemini acme-gemini

# Switch
aisw use claude work
aisw use --all --profile personal    # switch all tools at once
aisw context use acme

# Inspect
aisw status
aisw status --context
aisw list
```

## Profiles vs contexts

**Profile** means one saved account for one tool.

Use a profile when the problem is:
- "I need two Claude accounts and I want to switch between them safely."
- "I want a named Codex API key for CI."
- "I need to capture the Gemini account I am already logged into."

What you get from a profile:
- A stable name for one tool's auth state.
- Atomic switching and rollback for that tool.
- Clear per-tool status and backup behavior.

What you do not get from a profile alone:
- A cross-tool work mode when profile names differ across Claude, Codex, and Gemini.

**Context** means one saved multi-tool work mode built from profiles.

Use a context when the problem is:
- "My acme workspace uses one Claude account, a different OpenAI/Codex account, and a different Gemini account."
- "I want one command for work, personal, client-acme, or oss even when the per-tool profile names do not match."

What you get from a context:
- One user-facing name for a real mixed-account setup.
- Transactional multi-tool activation across the mapped tools.
- A clearer status view for whether your current tool state still matches a saved work mode.

What you do not get from a context:
- New credential storage or vendor auth behavior. Contexts only point at existing profiles.

The practical value is simple: `aisw use --all --profile personal` works when names line up, and `aisw context use acme` works when the real world does not.

## What it supports

| Tool | Binary | Auth methods | macOS | Linux | Windows |
|---|---|---|---|---|---|
| Claude Code | `claude` | OAuth, API key | Full | Full | Full |
| Codex CLI | `codex` | OAuth, API key | Full | Full | Full |
| Gemini CLI | `gemini` | OAuth, API key | Full | Full | Full |

Credentials are stored in the native OS keyring where available (macOS Keychain, Linux Secret Service, Windows Credential Manager) and fall back to encrypted local files with `0600` permissions.

## Command reference

```text
aisw init [--yes]
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--credential-backend file|system-keyring] [--set-active] [--yes]
aisw context create <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>]
aisw context list [--search TEXT] [--json]
aisw context use <name> [--state-mode isolated|shared] [--emit-env]
aisw context set <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>]
aisw context unset <name> [--claude] [--codex] [--gemini]
aisw context remove <name> [--yes]
aisw context rename <old> <new>
aisw use <tool> <profile> [--state-mode isolated|shared] [--emit-env]
aisw use --all --profile <profile> [--emit-env]
aisw workspace bind [PATH] --context <name>
aisw workspace bind --git-remote <PATTERN> --context <name>
aisw workspace bind --default --context <name>
aisw workspace status [--json]
aisw workspace doctor [--json]
aisw workspace guard --mode warn|strict
aisw list [tool] [--json]
aisw status [--context] [--json]
aisw remove <tool> <profile> [--yes] [--force]
aisw rename <tool> <old> <new>
aisw backup list [--json]
aisw backup restore <backup_id> [--yes]
aisw uninstall [--dry-run] [--remove-data] [--yes]
aisw shell-hook <bash|zsh|fish|pwsh>
aisw doctor [--json]
aisw verify [--json]
```

## Security

Credentials never leave the local machine. There is no remote service, no telemetry, and no credential proxy. All profile files are written with `0600` permissions. OS keyring integration uses the platform-native API directly. See [Security](https://burakdede.github.io/aisw/security/) for the full posture.

## Documentation

- [Quickstart](https://burakdede.github.io/aisw/quickstart/)
- [Commands](https://burakdede.github.io/aisw/commands/)
- [Workspace guardrails](https://burakdede.github.io/aisw/workspace/)
- [How it works](https://burakdede.github.io/aisw/how-it-works/)
- [Security](https://burakdede.github.io/aisw/security/)
- [Automation and scripting](https://burakdede.github.io/aisw/automation/)
- [Troubleshooting](https://burakdede.github.io/aisw/troubleshooting/)

## License

MIT.
