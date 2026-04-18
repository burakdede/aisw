# aisw

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-mark-dark.svg">
    <img src="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-mark-light.svg" alt="aisw logo" width="160" />
  </picture>
</p>

<p align="center">Account manager and account switcher for Claude Code, Codex CLI, and Gemini CLI.</p>

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
    <img src="https://img.shields.io/badge/docs-website-4c6fff?style=flat-square" alt="Documentation website" />
  </a>
</p>

`aisw` stores named profiles under `~/.aisw/` and applies the selected profile to each tool's live config.

## Demo

![aisw CLI demo](website/public/demos/aisw-important-workflows.gif)

## Install

```sh
# Homebrew
brew tap burakdede/tap
brew install aisw

# or shell installer (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh

# or Cargo
cargo install aisw
```

## Quickstart

```sh
# First-time setup
aisw init

# Add profiles
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex personal --api-key "$OPENAI_API_KEY"

# Switch
aisw use claude work

# Inspect
aisw status
aisw list
```

## Command surface

```text
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--label TEXT] [--set-active]
aisw use <tool> <profile> [--state-mode isolated|shared]
aisw use --all --profile <profile>
aisw list [tool] [--json]
aisw status [--json]
aisw remove <tool> <profile> [--yes] [--force]
aisw rename <tool> <old> <new>
aisw backup list [--json]
aisw backup restore <backup_id> [--yes]
aisw init [--yes]
aisw uninstall [--dry-run] [--remove-data] [--yes]
aisw shell-hook <bash|zsh|fish>
aisw doctor [--json]
```

## Supported tools

| Tool | Binary | Auth methods |
|---|---|---|
| Claude Code | `claude` | OAuth, API key |
| Codex CLI | `codex` | OAuth, API key |
| Gemini CLI | `gemini` | OAuth, API key |

## Docs

- [Quickstart](https://burakdede.github.io/aisw/quickstart/)
- [Commands](https://burakdede.github.io/aisw/commands/)
- [Automation](https://burakdede.github.io/aisw/automation/)
- [Troubleshooting](https://burakdede.github.io/aisw/troubleshooting/)

## License

MIT.
