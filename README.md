# aisw

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-logo.png">
    <img src="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-logo.png" alt="aisw" width="160" />
  </picture>
</p>

<p align="center"><strong>Named profile and context manager for Claude Code, Codex CLI, and Gemini CLI.</strong></p>

<p align="center">Switch between work, personal, and client accounts without copying auth files, editing hidden config, or logging in again every time.</p>

<p align="center"><em>The answer to "how do I switch between two Claude Code accounts?" and "how do I keep the right coding agent profile active per repo?"</em></p>

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

## Why people use aisw

`aisw` exists for a very specific kind of mess:

- You use one Claude Code account for work and another for personal projects.
- Codex CLI should use one OpenAI account for client A and a different one for client B, without relying on copied shared ChatGPT session files.
- Gemini CLI is already logged in, but you want to capture that state safely and switch back to it later.
- Your repo should open with the right coding agent account active, not whatever happened to be left over from the last project.

The underlying problem is not just "multiple accounts." It is that each upstream CLI stores auth differently, in different places, with different side effects. Manual switching usually means editing hidden files, copying `auth.json`, juggling `CLAUDE_CONFIG_DIR`, or hoping the shell session you are in still has the right environment.

`aisw` turns that into a named workflow:

- Save each account as a profile.
- Group mixed per-tool profiles into a context when real-world names do not line up.
- Switch in one command with rollback if something fails.
- Bind repos to expected contexts so the wrong account does not silently launch in the wrong workspace.

If you have ever searched for "Claude Code account switcher", "multiple Codex CLI accounts", "Gemini CLI work and personal profiles", or "coding agent profile switch per repo", this is the tool that addresses that workflow directly.

## Common situations

### I need separate work and personal accounts

Store both once, then switch explicitly instead of logging out and back in:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal
aisw use claude work
```

The same pattern works for Codex CLI and Gemini CLI. For Codex ChatGPT-managed auth, the durable model is one isolated `CODEX_HOME` per profile.

### I work across multiple clients

Each client can have its own Claude, Codex, and Gemini profiles, even when the names differ:

```sh
aisw context create client-acme \
  --claude acme-claude \
  --codex acme-codex \
  --gemini acme-gemini

aisw context use client-acme
```

That gives you one switch for the actual work mode instead of forcing fake naming symmetry across tools.

### I want the right profile active in the right repo

Bind the repo to a context and let the shell hook warn or block when the wrong account is active:

```sh
aisw workspace bind . --context client-acme
aisw workspace guard --mode strict
```

This is the practical answer to "how do I avoid opening a client repository with my personal coding agent account?"

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

## Quick start

```sh
# Bootstrap: creates ~/.aisw/, offers shell-hook setup,
# and can import already logged-in accounts
aisw init

# Store profiles for each tool
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

For GUI or other subprocess-driven clients, `aisw` also exposes machine-oriented commands such as:

```sh
aisw version --json
aisw capabilities --json
aisw add claude work --api-key-stdin --json
aisw add claude personal --progress-json
aisw verify --json
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

## Why aisw works better than manual switching

- It writes the native upstream credential locations that Claude Code, Codex CLI, and Gemini CLI already use.
- It snapshots live state before switching and rolls back on failure instead of leaving you mid-edit.
- It keeps stored profile data under `~/.aisw/` and uses the OS keyring where the platform supports it.
- It gives you one place to inspect active profile state, drift, warnings, backups, and workspace expectations.
- It stays local. No daemon, no remote control plane, no credential proxy.

## What it supports

| Tool | Binary | Auth methods | macOS | Linux | Windows |
|---|---|---|---|---|---|
| Claude Code | `claude` | OAuth, API key | Full | Full | Full |
| Codex CLI | `codex` | OAuth, API key | Full | Full | Full |
| Gemini CLI | `gemini` | OAuth, API key | Full | Full | Full |

Credentials are stored in the native OS keyring where available (macOS Keychain, Linux Secret Service, Windows Credential Manager) and fall back to encrypted local files with `0600` permissions.

For Codex specifically:
- Durable: API-key profiles.
- Durable: ChatGPT-managed profiles authenticated directly inside their own isolated `CODEX_HOME`.
- Bootstrap only: `aisw add codex <name> --from-live` for ChatGPT-managed auth.
- Unsupported: shared-mode ChatGPT auth switching.

## Command reference

```text
aisw init [--yes]
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--credential-backend file|system-keyring] [--set-active] [--yes]
aisw context create <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>] [--json]
aisw context list [--search TEXT] [--json]
aisw context use <name> [--state-mode isolated|shared] [--emit-env] [--json]
aisw context set <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>] [--json]
aisw context unset <name> [--claude] [--codex] [--gemini] [--json]
aisw context remove <name> [--yes] [--json]
aisw context rename <old> <new> [--json]
aisw use <tool> <profile> [--state-mode isolated|shared] [--emit-env]
aisw use --all --profile <profile> [--emit-env]
aisw workspace bind [PATH] --context <name> [--json]
aisw workspace bind --git-remote <PATTERN> --context <name> [--json]
aisw workspace bind --default --context <name> [--json]
aisw workspace unbind [PATH] [--json]
aisw workspace unbind --git-remote <PATTERN> [--json]
aisw workspace unbind --default [--json]
aisw workspace status [--json]
aisw workspace doctor [--json]
aisw workspace guard --mode warn|strict [--json]
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
aisw repair [--json] [--dry-run|--apply] [--fix home,permissions]
aisw project-bindings list [--json]
```

## Security

Credentials never leave the local machine. There is no remote service, no telemetry, and no credential proxy. All profile files are written with `0600` permissions. OS keyring integration uses the platform-native API directly. See [Security](https://burakdede.github.io/aisw/security/) for the full posture.

## Documentation

- [Common switching situations](https://burakdede.github.io/aisw/common-situations/)
- [Quickstart](https://burakdede.github.io/aisw/quickstart/)
- [Commands](https://burakdede.github.io/aisw/commands/)
- [Why aisw](https://burakdede.github.io/aisw/why-aisw/)
- [Workspace guardrails](https://burakdede.github.io/aisw/workspace/)
- [How it works](https://burakdede.github.io/aisw/how-it-works/)
- [Security](https://burakdede.github.io/aisw/security/)
- [Automation and scripting](https://burakdede.github.io/aisw/automation/)
- [Troubleshooting](https://burakdede.github.io/aisw/troubleshooting/)

## License

MIT.
