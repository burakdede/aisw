# aisw documentation

`aisw` stands for AI Switcher. It is a multi-account manager and account switcher for Claude Code, Codex CLI, and Gemini CLI, built to help you switch AI CLI accounts without manually copying credential files, editing config directories, or re-running login flows every time you hit a usage limit.

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

### What does aisw actually change when I switch accounts?

`aisw use` applies the selected profile into the live config location that Claude Code, Codex CLI, or Gemini CLI already reads. It does not patch the tool binary, install a proxy, or change anything outside the relevant local credential and config files.

### Does aisw send credentials or prompts over the network?

No. `aisw` itself does not proxy requests, inspect prompts, or send your credentials to a remote service. It is a local credential and profile switcher.

### Where are profiles stored, and how are they protected?

Stored profiles live under `~/.aisw/profiles/<tool>/<name>/`. Credential files are written with `0600` permissions so only your user can read or write them, and `aisw status` reports files that are broader than that.

### Can I use this for work, personal, and backup accounts across different tools?

Yes. A common setup is separate work, personal, client, or backup profiles for Claude Code, Codex CLI, and Gemini CLI so you can switch in seconds when a quota runs out or you need a different subscription.
