# Auth Storage Matrix

This document summarizes the default auth storage, profile/config locations, and other important local files for Claude Code, Codex CLI, and Gemini CLI across macOS, Linux/WSL, and Windows.

It separates vendor-documented behavior from places where the storage model is still partly inferred from implementation details or directly observed in real installs.

## Matrix

| Agent | Platform | Default auth storage | Auth files / locations | Settings / profile-like config | Session / history | Other important files | Confidence |
|---|---|---|---|---|---|---|---|
| Claude Code | macOS | macOS Keychain for API keys, OAuth tokens, and other credentials | Docs explicitly say credentials are in the encrypted macOS Keychain; docs also say `~/.claude.json` stores OAuth session plus user/local MCP config, per-project state, and caches | `~/.claude/settings.json`, `.claude/settings.json`, `.claude/settings.local.json`, managed settings under `/Library/Application Support/ClaudeCode/` or MDM prefs | Docs confirm transcript persistence as `.jsonl` files exists, but do not give the exact default transcript directory on the page checked | `~/.claude/agents/`, `~/.claude/CLAUDE.md`, `~/.claude.json`, `.mcp.json` | High for auth/config; medium for exact transcript location |
| Claude Code | Linux / WSL | Docs say credentials are "stored securely," but do not explicitly spell out the backend the way they do for macOS; observed local installs use `~/.claude/.credentials.json` | Observed live auth file: `~/.claude/.credentials.json`; docs explicitly document `~/.claude.json` for OAuth session metadata/caches/per-project state | `~/.claude/settings.json`, `.claude/settings.json`, `.claude/settings.local.json`, managed settings under `/etc/claude-code/` | Docs confirm persisted `.jsonl` transcripts via `cleanupPeriodDays`, but not exact directory on the page checked | `~/.claude/agents/`, `~/.claude/CLAUDE.md`, `~/.claude.json`, `.mcp.json` | Medium: runtime-confirmed auth file, weaker vendor documentation |
| Claude Code | Windows | Docs say credentials are "stored securely," but only explicitly name Keychain for macOS | Same note as Linux: `~/.claude.json` is documented for OAuth session/config/cache state, but not as the secure credential backend itself | `~/.claude/settings.json`, `.claude/settings.json`, `.claude/settings.local.json`, managed settings via registry or `C:\Program Files\ClaudeCode\managed-settings.json` | Docs confirm persisted `.jsonl` transcripts, exact default path not stated on the page checked | `~/.claude/agents/`, `~/.claude/CLAUDE.md`, `~/.claude.json`, `.mcp.json` | Medium |
| Codex CLI | macOS | `auto` by default in practice if configured that way; docs say credentials are cached either in `~/.codex/auth.json` or the OS credential store | `~/.codex/auth.json` when file-backed; OS credential store when `cli_auth_credentials_store = "keyring"`; `auto` prefers OS credential store when available | `~/.codex/config.toml`, `.codex/config.toml`, optional config profiles inside TOML via `--profile <name>` | Docs checked do not explicitly document the session/history file path | `CODEX_HOME` overrides root; CLI and IDE share cached login details | High |
| Codex CLI | Linux | Docs support file or OS credential store; observed local installs use file-backed `~/.codex/auth.json` | `~/.codex/auth.json` or OS credential store | `~/.codex/config.toml`, `.codex/config.toml`, `/etc/codex/config.toml` | Not explicitly documented in the pages checked | `CODEX_HOME` root override | High |
| Codex CLI | Windows | Same documented model as macOS/Linux: file or OS credential store | `~/.codex/auth.json` or OS credential store | `~/.codex/config.toml`, `.codex/config.toml`; Windows-specific sandbox settings live in the same TOML | Not explicitly documented in the pages checked | `CODEX_HOME` root override | High |
| Gemini CLI | macOS | Default recommended interactive auth is Google sign-in; credentials are "cached locally for future sessions" | OAuth cache is local under `~/.gemini/` by implementation and surrounding docs; API key mode uses `GEMINI_API_KEY`; MCP OAuth tokens are in `~/.gemini/mcp-oauth-tokens.json` | `~/.gemini/settings.json`, `.gemini/settings.json` | `~/.gemini/tmp/<project_hash>/chats/` | `~/.gemini/mcp-server-enablement.json`; MCP configs live in `settings.json` | High |
| Gemini CLI | Linux | Docs say credentials are cached locally; observed local installs use `~/.gemini/settings.json` and `~/.gemini/oauth_creds.json` for OAuth state | `~/.gemini/settings.json`, `~/.gemini/oauth_creds.json`, plus other local OAuth/cache files under `~/.gemini/` | `~/.gemini/settings.json`, `.gemini/settings.json` | `~/.gemini/tmp/<project_hash>/chats/` | `~/.gemini/mcp-oauth-tokens.json`, `~/.gemini/mcp-server-enablement.json` | High |
| Gemini CLI | Windows | Same documented auth choices; docs show Windows env-var examples for API key and Vertex AI | Docs use `~/.gemini/...` notation rather than spelling the native Windows expansion, but behavior is presented as the same logical home-relative locations | `~/.gemini/settings.json`, `.gemini/settings.json` | `~/.gemini/tmp/<project_hash>/chats/` | `~/.gemini/mcp-oauth-tokens.json`, `~/.gemini/mcp-server-enablement.json` | High |

## Default Storage Method Summary

### Claude Code

- macOS: Keychain is the documented default secure credential store.
- Linux: vendor docs do not state the exact backend as clearly, but observed installs use `~/.claude/.credentials.json` for live auth. Config/state is still split across `~/.claude/`, project `.claude/`, and `~/.claude.json`.
- Windows: vendor docs do not state the exact backend as clearly; config/state is definitely split across `~/.claude/`, project `.claude/`, and `~/.claude.json`.

### Codex CLI

- All platforms: docs explicitly support `file`, `keyring`, and `auto`.
- `auto` means OS credential store when available, otherwise `~/.codex/auth.json`.
- This is the clearest documented cross-platform auth-storage model of the three.
- Observed Linux installs can still be file-backed in practice, with auth stored in `~/.codex/auth.json`.

### Gemini CLI

- All platforms: docs frame auth as either browser sign-in with locally cached credentials, env-var/API-key auth, or Vertex AI/ADC/service-account auth.
- Settings and most local state are home-relative under `~/.gemini/`.
- Observed Linux installs store OAuth state in `~/.gemini/settings.json` and `~/.gemini/oauth_creds.json`.

## Important Implications for `aisw`

- Claude import reliability is strongest on macOS because Anthropic explicitly documents Keychain storage there.
- Codex import should support both `~/.codex/auth.json` and OS credential-store-backed sessions on every platform, because OpenAI explicitly documents both.
- Gemini import should treat `~/.gemini/` as the important state root, not just one file. That includes OAuth cache files, settings, session data, and MCP OAuth token files depending on feature usage.

## `aisw` Support Status

This section is the product policy layer on top of the vendor storage facts above.

| Tool | Backend / situation | `aisw init` import | `aisw use` switching | Current policy |
|---|---|---|---|---|
| Claude Code | file-backed live credentials | Supported | Supported | fully supported |
| Claude Code | readable system-keyring-backed live credentials | Supported | Supported | fully supported |
| Claude Code | local metadata only, no readable live auth | Not importable | Not applicable | reported explicitly, no guessing |
| Codex CLI | file-backed `auth.json` | Supported | Supported | fully supported |
| Codex CLI | readable system-keyring-backed live credentials | Supported | Supported if the live keyring account can be discovered | supported with account-discovery requirement |
| Codex CLI | system-keyring-backed auth with no discoverable live account | Not importable today | Fail-closed | `aisw` will not guess a username-based keyring account |
| Gemini CLI | file-managed auth and local state under `~/.gemini` | Supported | Supported | fully supported |
| Gemini CLI | system keyring | Not supported | Not supported | Gemini remains file-managed in `aisw` |

Notes:
- `fail-closed` means `aisw` refuses to fabricate a storage target that it cannot prove the upstream CLI will read.
- for Codex, this matters because real keyring account names can be opaque `cli|...` identifiers rather than the OS username.
- for Claude on Linux and Windows, upstream docs still describe secure storage less explicitly than Codex, so support confidence is lower even where the implementation path is in place.

## Documentation Gaps

- Claude: Linux/Windows credential backend is not spelled out as explicitly as macOS Keychain, even though Linux file-backed auth has been observed in real installs.
- Codex: auth storage is documented well, but session/history file paths were not clearly documented on the pages checked.
- Gemini: core auth/config/session locations are documented well; exact non-MCP OAuth cache filenames are implied by docs and implementation more than centrally specified in one canonical table.

## Sources

- Claude settings: <https://code.claude.com/docs/en/settings>
- Claude IAM / credential management: <https://code.claude.com/docs/en/team>
- Codex auth: <https://developers.openai.com/codex/auth>
- Codex config basics: <https://developers.openai.com/codex/config-basic>
- Gemini auth: <https://geminicli.com/docs/get-started/authentication/>
- Gemini settings: <https://geminicli.com/docs/cli/settings/>
- Gemini sessions: <https://geminicli.com/docs/cli/session-management/>
- Gemini MCP OAuth token storage: <https://geminicli.com/docs/tools/mcp-server/>
