---
title: aisw — AI coding agent account manager
description: aisw manages named profiles for Claude Code, Codex CLI, and Gemini CLI. Switch between work, personal, and client accounts with one command on macOS, Linux, and Windows.
---

# aisw

Named profile manager for Claude Code, Codex CLI, and Gemini CLI. Store and switch between multiple accounts in one command across all three AI coding agents — on macOS, Linux, and Windows.

## Install

```sh
brew tap burakdede/tap && brew install aisw
```

Other installers:

```sh
# Shell installer (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh

# Cargo
cargo install aisw
```

## First run

```sh
aisw init
```

`init` creates `~/.aisw/`, configures the optional shell hook, and offers to import any currently logged-in tool accounts so you start with zero manual re-authentication.

## Core workflow

```sh
# Store profiles
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal              # launches interactive OAuth
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"

# Activate a profile
aisw use claude work
aisw use --all --profile personal     # switch all tools at once

# Inspect state
aisw status
aisw list
```

## Start here

1. [Quickstart](quickstart.md) — install, first profile, first switch
2. [Commands](commands.md) — complete syntax and flag reference
3. [How it works](how-it-works.md) — design decisions, credential storage, platform behavior
4. [Security](security.md) — local-only storage, keyring integration, file permissions
5. [Automation and scripting](automation.md) — CI patterns, JSON output, non-interactive mode
6. [Troubleshooting](troubleshooting.md) — common failures and diagnostics

## Additional reference

- [Adding profiles](adding-profiles.md)
- [Shell integration](shell-integration.md)
- [Supported tools](supported-tools.md)
- [Configuration](config.md)
- [Changelog](https://github.com/burakdede/aisw/releases)
