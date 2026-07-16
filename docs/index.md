---
title: aisw  -  AI coding agent account manager
description: aisw manages named profiles for Claude Code, Codex CLI, and Gemini CLI. Switch between multiple work, personal, and client accounts with one command. Supports macOS, Linux, and Windows.
---

# aisw

Named profile and context manager for Claude Code, Codex CLI, and Gemini CLI. Store per-tool accounts, save mixed-name work modes, and switch between them in one command across all three AI coding agents  -  on macOS, Linux, and Windows.

If you maintain separate work and personal accounts for Claude Code, Codex, or Gemini  -  or manage credentials for multiple clients  -  `aisw` gives you one command to switch instead of manually editing `~/.claude/.credentials.json`, juggling `CLAUDE_CONFIG_DIR` overrides, or copying `auth.json` files between directories. For Codex ChatGPT-managed auth, the durable model is one isolated `CODEX_HOME` per profile, not copied shared session state.

It is built for the questions people actually ask:

- How do I switch between two Claude Code accounts?
- How do I keep separate Codex CLI accounts for different clients?
- How do I store work and personal Gemini CLI profiles on one machine?
- How do I make sure the right coding agent account is active in the right repo?

`aisw` answers those with three primitives:

- Profiles: named saved accounts for one tool
- Contexts: one saved work mode across multiple tools
- Workspace guardrails: repo-aware warnings or blocks before launching the wrong account

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

# Save and activate a mixed-name context
aisw context create acme --claude acme-claude --codex acme-codex --gemini acme-gemini
aisw context use acme

# Inspect state
aisw status
aisw status --context
aisw list
```

## Common situations

### Work and personal accounts for the same tool

Store both once, then switch by name:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal
aisw use claude work
```

### Mixed client setup across Claude, Codex, and Gemini

Use a context when each tool needs a different profile name:

```sh
aisw context create client-acme \
  --claude acme-claude \
  --codex client-a-openai \
  --gemini gemini-consulting

aisw context use client-acme
```

### Wrong-account protection per repo

Use workspace guardrails when the repo itself should enforce the right work mode:

```sh
aisw workspace bind . --context client-acme
aisw workspace guard --mode strict
```

## Start here

1. [Quickstart](quickstart.md)  -  install, first profile, first switch
2. [Common switching situations](common-situations.md)  -  work/personal, client, repo guardrails
3. [Commands](commands.md)  -  complete syntax and flag reference
4. [How it works](how-it-works.md)  -  design decisions, credential storage, platform behavior
5. [Security](security.md)  -  local-only storage, keyring integration, file permissions
6. [Automation and scripting](automation.md)  -  CI patterns, JSON output, non-interactive mode
7. [Troubleshooting](troubleshooting.md)  -  common failures and diagnostics

## Additional reference

- [Adding profiles](adding-profiles.md)
- [Shell integration](shell-integration.md)
- [Workspace guardrails](workspace.md)
- [Why aisw](why-aisw.md)
- [Supported tools](supported-tools.md)
- [Configuration](config.md)
- [Changelog](https://github.com/burakdede/aisw/releases)
