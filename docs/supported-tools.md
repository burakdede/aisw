---
title: Supported tools
description: Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI support matrix  -  auth methods, credential locations, OS keyring support, and state mode behavior per platform.
---

# Supported tools

`aisw` supports three AI coding agent CLIs:

| Tool | Binary | Auth methods | macOS | Linux | Windows |
|---|---|---|---|---|---|
| Claude Code | `claude` | OAuth, API key | Full | Full | Full |
| Codex CLI | `codex` | OAuth, API key | Full | Full | Full |
| Gemini CLI | `gemini` | Google-account auth, Vertex AI, API key | Full | Full | Full |
| Antigravity CLI | `agy` | OAuth | Full | Full | Full |

## Binary detection

`aisw` resolves each tool from `PATH` and confirms it is present by running `<binary> --version`. If a binary is not found, `aisw status` reports it as missing and `aisw use` for that tool is blocked with an error.

## State mode

| Tool | `isolated` (default) | `shared` |
|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR` set to profile directory when the install supports profile-owned auth | `CLAUDE_CONFIG_DIR` unset |
| Codex CLI | `CODEX_HOME` set to profile directory | `CODEX_HOME` unset for API-key profiles only |
| Gemini CLI | Profile files applied to `~/.gemini/` | Not supported |
| Antigravity CLI | Not supported | Shared live keyring-backed auth and `~/.gemini` config roots restored transactionally |

In `isolated` mode, the tool reads config, history, and extensions from the profile-specific directory. In `shared` mode, the tool reads its standard config directory. Credentials are applied to the live location in both modes; state mode only controls which config directory the tool reads.

For Codex ChatGPT-managed auth, shared mode is intentionally unsupported. Use one isolated `CODEX_HOME` per profile and authenticate each profile independently.

For Claude OAuth, isolated mode is intentionally blocked only when Claude is using its legacy shared live Keychain credential. `CLAUDE_CONFIG_DIR` still isolates config/history in that case, but not the underlying OAuth credential owner. Use shared mode for that profile, or prefer API key / long-lived token flows for repeatable switching.

Gemini does not support `shared` mode because its auth state and broader local state (settings, session history, MCP configs) are tightly coupled under `~/.gemini/`. Separating them is not safely possible without risking session corruption.

Antigravity does not currently expose a documented per-profile auth/data root like `CODEX_HOME` or `CLAUDE_CONFIG_DIR`. `aisw` therefore supports Antigravity through shared live switching: it restores the live OS keyring credential plus the documented `~/.gemini/antigravity-cli/` and `~/.gemini/config/` trees for the selected profile.

## Credential storage by tool and platform

### Claude Code

| Platform | Live credential location | Keyring |
|---|---|---|
| macOS | `~/.claude/.credentials.json` or macOS Keychain | Supported  -  preferred for OAuth-based accounts |
| Linux | `~/.claude/.credentials.json` | Supported via Secret Service |
| Windows | `~/.claude/.credentials.json` | Supported via Windows Credential Manager |

OAuth account metadata (display name, organization) is stored in `~/.claude.json` under the `oauthAccount` key. `aisw` captures and restores this alongside credentials.

Claude Code also stores MCP OAuth tokens in the credentials payload. `aisw` preserves the full credential payload including `mcpOAuth` keys when writing to any backend.

Supported Claude auth models in `aisw`:
- Durable: API-key profiles.
- Durable: file-backed OAuth where the live credential file follows `CLAUDE_CONFIG_DIR`.
- Durable: OAuth installs whose keychain credential is scoped by `CLAUDE_CONFIG_DIR`.
- Supported but not isolated: OAuth profiles backed by Claude's legacy shared live Keychain credential.
- Caution: if Claude's keychain behavior cannot be determined, `aisw` warns that isolated switching may not be durable on that install.

### Codex CLI

| Platform | Live credential location | Keyring |
|---|---|---|
| macOS | `~/.codex/auth.json` or OS keyring | Supported |
| Linux | `~/.codex/auth.json` or OS keyring | Supported via Secret Service |
| Windows | `~/.codex/auth.json` or OS keyring | Supported via Windows Credential Manager |

Codex uses `CODEX_HOME` to override its root directory. `aisw` sets this variable when applying profiles in isolated mode, which causes Codex to read its complete state (auth, config, history) from the profile directory.

Supported Codex auth models in `aisw`:
- Durable: API-key profiles.
- Durable: ChatGPT-managed profiles authenticated directly inside their own isolated `CODEX_HOME`.
- Durable when already live upstream: personal access token sessions imported with `aisw add codex <name> --from-live` after `codex login --with-access-token`.
- Bootstrap only: ChatGPT-managed profiles imported with `aisw add codex <name> --from-live`.
- Unsupported: shared-mode ChatGPT auth switching for ChatGPT-managed refresh-token auth.

Codex's keyring account identifier is an opaque string, not the system username. `aisw` discovers the identifier from the live keyring entry during import and stores it so subsequent switches write to the correct account. `aisw` will not fabricate a keyring account name if it cannot read the live identifier.

### Gemini CLI

| Platform | Live credential location | Keyring |
|---|---|---|
| macOS | `~/.gemini/` (oauth_creds.json, settings.json, and other state files) | Not supported |
| Linux | `~/.gemini/` | Not supported |
| Windows | `~/.gemini/` | Not supported |

Gemini stores all auth and local state under `~/.gemini/`. `aisw` captures and restores the complete regular-file tree for that directory and removes stale live files from the previously active Gemini profile. This includes OAuth tokens, settings, and any MCP OAuth token files stored as regular files under the Gemini state root.

Upstream Gemini CLI docs currently recommend Google-account login for interactive local use. Some account types still require `GOOGLE_CLOUD_PROJECT`. `aisw` can manage:

- API-key-backed Gemini profiles (`GEMINI_API_KEY`)
- Vertex AI-backed Gemini profiles
- Google-account Gemini logins, including the standard local browser-login flow and Workspace / Code Assist-style flows that may require `GOOGLE_CLOUD_PROJECT`

For interactive Google-account / OAuth-style capture, `aisw` uses `GEMINI_CLI_HOME` to redirect Gemini's config root to a scratch directory during the login flow, then copies the resulting files into the profile. This was introduced in Gemini CLI as the clean way to redirect config storage without overriding `HOME`.

API key profiles store a `.env` file containing `GEMINI_API_KEY=<key>`. This is the format Gemini reads natively from `~/.gemini/.env`.

### Antigravity CLI

- Live auth: OS-native keyring entry (`service=gemini`, `account=antigravity`) as observed in upstream issue reports and docs-aligned behavior.
- Live config/state: `~/.gemini/antigravity-cli/` and `~/.gemini/config/`
- `--from-live`: captures the current live keyring-backed session plus both documented config roots.
- Interactive OAuth: launches `agy`, captures the resulting live keyring/config state, and restores the prior live state unless `--set-active` is requested.
- No API-key path in `aisw` because upstream Antigravity docs currently describe OAuth/keyring auth, not API-key profile auth.
- No `--state-mode` support because upstream does not currently document an isolated per-profile auth or data root.

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
| Antigravity CLI | File-backed managed profile + live OS keyring apply | Supported | Supported | Stores captured secret in the profile, then restores it into the live keyring on switch |
| Antigravity CLI | System keyring-backed managed profile | Supported | Supported | Stores the captured live keyring secret in `aisw`'s managed keyring backend |

**Fail-closed** means `aisw` refuses the operation rather than guessing. This applies specifically to Codex when the keyring account identifier cannot be read from the live credential store.

`aisw add --credential-backend ...` sets the managed profile backend only. It does not force the upstream CLI's live auth backend.

## References

- [Auth Storage Matrix](https://github.com/burakdede/aisw/blob/main/AUTH_STORAGE_MATRIX.md)  -  detailed research on credential file locations and storage models per tool and platform
- [How it works](how-it-works.md)  -  implementation details and design decisions
- [Security](security.md)  -  keyring integration, file permissions, and storage safety
