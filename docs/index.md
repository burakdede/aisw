# aisw documentation

`aisw` is a multi-account manager and account switcher for Claude Code, Codex CLI, and Gemini CLI. It helps you switch AI CLI accounts without manually copying credential files, editing config directories, or re-running login flows every time you hit a usage limit.

Use this documentation to install the tool, set up profiles, understand how switching works, and operate the release workflow.

## What aisw helps with

Developers usually find `aisw` when they are trying to solve one of these problems:

- switch between multiple Claude Code accounts
- switch between multiple Codex CLI accounts
- switch between multiple Gemini CLI accounts
- manage several AI CLI subscriptions on one machine
- rotate between work and personal AI coding tool profiles
- keep Claude, Codex, and Gemini credentials organized without manual file copying

If you were searching for an AI CLI account switcher, a multi-account CLI manager, or a way to manage multiple Claude, Codex, or Gemini logins locally, this documentation is the right place to start.

## Start here

| Document | Description |
|---|---|
| [Quickstart](quickstart.md) | Install aisw, run first-time setup, and switch accounts quickly |
| [Commands](commands.md) | Full reference for all subcommands and flags |
| [Adding Profiles](adding-profiles.md) | OAuth and API key auth flows per tool |

## Setup and operation

| Document | Description |
|---|---|
| [Shell Integration](shell-integration.md) | Shell hook setup for bash, zsh, fish |
| [Supported Tools](supported-tools.md) | Tool compatibility, binary names, auth methods |
| [Configuration](config.md) | `~/.aisw/config.json` schema and settings |

## Common questions

### Can aisw switch between multiple Claude Code accounts?

Yes. `aisw` can store and switch multiple Claude Code profiles, including API key and OAuth-based profiles.

### Can aisw manage both Codex CLI and Gemini CLI accounts too?

Yes. `aisw` supports Claude Code, Codex CLI, and Gemini CLI in one local profile manager.

### Does aisw proxy requests or inspect prompts?

No. `aisw` is a local credential and profile switcher. It does not proxy traffic, inspect prompts, or run a gateway service.

### Is this useful for work and personal accounts?

Yes. A common use case is keeping separate work, personal, client, or backup AI CLI accounts and switching between them quickly.
