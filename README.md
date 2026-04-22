# aisw

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-logo.png">
    <img src="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-logo.png" alt="aisw" width="160" />
  </picture>
</p>

<p align="center"><strong>Named profile manager for Claude Code, Codex CLI, and Gemini CLI.</strong></p>

<p align="center">Switch between work, personal, and client accounts in one command - across all three AI coding agents.</p>

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

`aisw` solves this with named profiles. Each profile is a captured snapshot of a tool's auth state. Switching is a single command that atomically replaces the live credentials and produces a clean rollback on failure.

## Demo

![aisw CLI demo](website/public/demos/aisw-important-workflows.gif)

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

# Switch
aisw use claude work
aisw use --all --profile personal    # switch all tools at once

# Inspect
aisw status
aisw list
```

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
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--set-active]
aisw use <tool> <profile> [--state-mode isolated|shared]
aisw use --all --profile <profile>
aisw list [tool] [--json]
aisw status [--json]
aisw remove <tool> <profile> [--yes] [--force]
aisw rename <tool> <old> <new>
aisw backup list [--json]
aisw backup restore <backup_id> [--yes]
aisw uninstall [--dry-run] [--remove-data] [--yes]
aisw shell-hook <bash|zsh|fish>
aisw doctor [--json]
```

## Security

Credentials never leave the local machine. There is no remote service, no telemetry, and no credential proxy. All profile files are written with `0600` permissions. OS keyring integration uses the platform-native API directly. See [Security](https://burakdede.github.io/aisw/security/) for the full posture.

## Documentation

- [Quickstart](https://burakdede.github.io/aisw/quickstart/)
- [Commands](https://burakdede.github.io/aisw/commands/)
- [How it works](https://burakdede.github.io/aisw/how-it-works/)
- [Security](https://burakdede.github.io/aisw/security/)
- [Automation and scripting](https://burakdede.github.io/aisw/automation/)
- [Troubleshooting](https://burakdede.github.io/aisw/troubleshooting/)

## License

MIT.
