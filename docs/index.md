---
title: aisw  -  AI coding agent account manager
description: aisw manages named profiles for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI. Switch between multiple work, personal, and client accounts with one command. Supports macOS, Linux, and Windows.
---

# aisw

Switch coding-agent accounts without copying credential files or logging in again every time.

`aisw` is a local profile and context manager for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI. Save the accounts you already use, activate one tool or an entire work mode with a single command, and add repo-aware guardrails when the wrong account would be costly.

It is useful when you:

- keep separate work, personal, or client accounts;
- use several coding agents whose profile names do not line up; or
- want each repository to declare which account context it expects.

The core model is deliberately small:

- **Profiles** save one tool's account state.
- **Contexts** map one work mode to profiles across tools.
- **Workspace guardrails** warn or block launches when the active context is wrong.

aisw applies native credential state locally, takes a rollback snapshot before a switch, and reports whether live state still matches the profile you selected. It does not proxy model traffic, run a daemon, or send credentials to a service.

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

If you want the guided path, continue with [Quickstart](quickstart.md). If you are evaluating whether aisw fits a more complex setup, start with [Common switching situations](common-situations.md).

## Core workflow

```sh
# Store profiles
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal              # launches interactive OAuth
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"
aisw add antigravity work             # shared OAuth
aisw add antigravity api --api-key "$GEMINI_API_KEY"

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

For the mechanics behind profiles, contexts, and rollback, see [How aisw works](how-it-works.md). For provider-specific auth behavior and platform limits, see [Supported tools](supported-tools.md).

## See the workflow

These recordings walk through actual commands in isolated demo environments. They are useful for seeing the shape of the workflow before you install; the linked guides explain every decision and edge case.

**Switch, inspect, and recover a profile**

![Profile lifecycle demo](https://burakdede.github.io/aisw/demos/aisw-important-workflows.gif)

[Follow the Quickstart](quickstart.md) for the shortest path, or read [Adding profiles](adding-profiles.md) when you need to choose between OAuth, API keys, environment variables, and live imports.

**Group differently named accounts into one client context**

![Context workflow demo](https://burakdede.github.io/aisw/demos/aisw-context-workflow.gif)

[Learn about contexts](common-situations.md) when one work mode spans multiple providers.

**Prevent a wrong-account launch in a repository**

![Workspace guardrails demo](https://burakdede.github.io/aisw/demos/aisw-workspace-workflow.gif)

[Set up workspace guardrails](workspace.md) when the account associated with a repository matters as much as the code itself.

## Common situations

### Work and personal accounts for the same tool

Store both once, then switch by name:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal
aisw use claude work
```

See [Adding profiles](adding-profiles.md) for API keys, OAuth, live imports, and duplicate-account behavior.

### Mixed client setup across Claude, Codex, Gemini, and Antigravity

Use a context when each tool needs a different profile name:

```sh
aisw context create client-acme \
  --claude acme-claude \
  --codex client-a-openai \
  --gemini gemini-consulting \
  --antigravity acme-agy

aisw context use client-acme
```

A context only needs the tools you actually use  -  map one, or all four.

See [Common switching situations](common-situations.md) for the profile-versus-context decision and [Configuration](config.md) for the files and schemas involved.

### Wrong-account protection per repo

Use workspace guardrails when the repo itself should enforce the right work mode:

```sh
aisw workspace bind . --context client-acme
aisw workspace guard --mode strict
```

See [Workspace guardrails](workspace.md) for resolution order, remote patterns, path rules, and shell-hook behavior.

## Start here

| If you want to... | Start here | Then go deeper |
| --- | --- | --- |
| Install and switch your first account | [Quickstart](quickstart.md) | [Adding profiles](adding-profiles.md) |
| Choose between profiles, contexts, and guardrails | [Common switching situations](common-situations.md) | [Workspace guardrails](workspace.md) |
| Integrate aisw with a GUI, script, or CI job | [Automation and scripting](automation.md) | [Commands](commands.md) |
| Understand storage, rollback, and platform behavior | [How aisw works](how-it-works.md) | [Security](security.md) |
| Check support before installing | [Supported tools](supported-tools.md) | [Acceptance matrix](acceptance-matrix.md) |
| Diagnose a mismatch or failed switch | [Troubleshooting](troubleshooting.md) | [Configuration](config.md) |

## Additional reference

- [Adding profiles](adding-profiles.md)
- [Shell integration](shell-integration.md)
- [Workspace guardrails](workspace.md)
- [Why aisw](why-aisw.md)
- [Supported tools](supported-tools.md)
- [Configuration](config.md)
- [Changelog](https://github.com/burakdede/aisw/releases)
