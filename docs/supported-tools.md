---
title: Supported tools
description: Claude Code, Codex CLI, and Gemini CLI support matrix — auth methods, credential locations, OS keyring support, and state mode behavior per platform.
---

# Supported tools

`aisw` supports three AI coding agent CLIs:

| Tool | Binary | Auth methods | macOS | Linux | Windows |
|---|---|---|---|---|---|
| Claude Code | `claude` | OAuth, API key | Full | Full | Full |
| Codex CLI | `codex` | OAuth, API key | Full | Full | Full |
| Gemini CLI | `gemini` | OAuth, API key | Full | Full | Full |

## Binary detection

`aisw` resolves each tool from `PATH` and confirms it is present by running `<binary> --version`. If a binary is not found, `aisw status` reports it as missing and `aisw use` for that tool is blocked with an error.

## State mode

| Tool | `isolated` (default) | `shared` |
|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR` set to profile directory | `CLAUDE_CONFIG_DIR` unset |
| Codex CLI | `CODEX_HOME` set to profile directory | `CODEX_HOME` unset |
| Gemini CLI | Profile files applied to `~/.gemini/` | Not supported |

In `isolated` mode, the tool reads config, history, and extensions from the profile-specific directory. In `shared` mode, the tool reads its standard config directory. Credentials are applied to the live location in both modes; state mode only controls which config directory the tool reads.

Gemini does not support `shared` mode because its auth state and broader local state (settings, session history, MCP configs) are tightly coupled under `~/.gemini/`. Separating them is not safely possible without risking session corruption.

## Credential storage by tool and platform

### Claude Code

| Platform | Live credential location | Keyring |
|---|---|---|
| macOS | `~/.claude/.credentials.json` or macOS Keychain | Supported — preferred for OAuth-based accounts |
| Linux | `~/.claude/.credentials.json` | Supported via Secret Service |
| Windows | `~/.claude/.credentials.json` | Supported via Windows Credential Manager |

OAuth account metadata (display name, organization) is stored in `~/.claude.json` under the `oauthAccount` key. `aisw` captures and restores this alongside credentials.

Claude Code also stores MCP OAuth tokens in the credentials payload. `aisw` preserves the full credential payload including `mcpOAuth` keys when writing to any backend.

### Codex CLI

| Platform | Live credential location | Keyring |
|---|---|---|
| macOS | `~/.codex/auth.json` or OS keyring | Supported |
| Linux | `~/.codex/auth.json` or OS keyring | Supported via Secret Service |
| Windows | `~/.codex/auth.json` or OS keyring | Supported via Windows Credential Manager |

Codex uses `CODEX_HOME` to override its root directory. `aisw` sets this variable when applying profiles in isolated mode, which causes Codex to read its complete state (auth, config, history) from the profile directory.

Codex's keyring account identifier is an opaque string, not the system username. `aisw` discovers the identifier from the live keyring entry during import and stores it so subsequent switches write to the correct account. `aisw` will not fabricate a keyring account name if it cannot read the live identifier.

### Gemini CLI

| Platform | Live credential location | Keyring |
|---|---|---|
| macOS | `~/.gemini/` (oauth_creds.json, settings.json, and other state files) | Not supported |
| Linux | `~/.gemini/` | Not supported |
| Windows | `~/.gemini/` | Not supported |

Gemini stores all auth and local state under `~/.gemini/`. `aisw` captures and restores the complete directory contents. This includes OAuth tokens, settings, and any MCP OAuth token files.

For interactive OAuth, `aisw` uses `GEMINI_CLI_HOME` to redirect Gemini's config root to a scratch directory during the login flow, then copies the resulting files into the profile. This was introduced in Gemini CLI as the clean way to redirect config storage without overriding `HOME`.

API key profiles store a `.env` file containing `GEMINI_API_KEY=<key>`. This is the format Gemini reads natively from `~/.gemini/.env`.

## Auth backend support matrix

| Tool | Backend | `aisw init` import | `aisw use` | Notes |
|---|---|---|---|---|
| Claude Code | File credentials | Supported | Supported | Standard on Linux/Windows |
| Claude Code | System keyring | Supported | Supported | Standard on macOS |
| Codex CLI | File `auth.json` | Supported | Supported | Available on all platforms |
| Codex CLI | System keyring (discoverable) | Supported | Supported | Requires readable live keyring entry |
| Codex CLI | System keyring (not discoverable) | Not supported | Fail-closed | `aisw` will not fabricate an account identifier |
| Gemini CLI | File-backed `~/.gemini/` state | Supported | Supported | Full directory capture and restore |
| Gemini CLI | System keyring | Not supported | Not supported | Gemini does not use keyring for credentials |

**Fail-closed** means `aisw` refuses the operation rather than guessing. This applies specifically to Codex when the keyring account identifier cannot be read from the live credential store.

## References

- [Auth Storage Matrix](https://github.com/burakdede/aisw/blob/main/AUTH_STORAGE_MATRIX.md) — detailed research on credential file locations and storage models per tool and platform
- [How it works](how-it-works.md) — implementation details and design decisions
- [Security](security.md) — keyring integration, file permissions, and storage safety
