---
title: How aisw works
description: Architecture, design decisions, credential storage model, OS keyring integration, and per-tool implementation details for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI.
---

# How aisw works

This page explains the design decisions behind `aisw`, how credentials are stored and applied, and the per-tool implementation details for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI.

## Profile and context model

`aisw` stores named profiles under `~/.aisw/profiles/<tool>/<name>/`. A profile is a captured snapshot of a tool's credential and auth state. The profile directory contains the credential files specific to that tool  -  nothing else.

Contexts are a higher-level sparse mapping from tool to profile name. They live in `~/.aisw/config.json` alongside the per-tool profile registry and let one saved name activate a mixed set such as `claude -> acme-claude`, `codex -> acme-codex`, `gemini -> acme-gemini`.

The central registry is `~/.aisw/config.json`, which records which profiles exist, which is active per tool, which contexts exist, and metadata such as auth method and credential backend. Credentials are never stored in `config.json`.

```text
~/.aisw/
├── config.json               # registry: active profiles, contexts, metadata
├── profiles/
│   ├── claude/work/          # credential files for this profile
│   ├── claude/personal/
│   ├── codex/work/
│   └── gemini/personal/
└── backups/                  # timestamped snapshots before remove or rename
```

When you run `aisw use claude work`, `aisw` reads the stored credential files for that profile and writes them to the locations Claude Code actually reads. The tool sees exactly what it would see if you had authenticated natively.

When you run `aisw context use acme`, `aisw` resolves every mapped tool/profile pair first, snapshots all affected live state, applies the mapped writes in deterministic tool order, and commits the config update only after the full activation succeeds.

## Atomic switching with rollback

Profile activation is transactional. Before writing any live credential file, `aisw` snapshots the current live state. If any write fails partway through, the snapshot is restored and an error is returned. You never end up with a partially switched account.

This matters most when a tool stores state across multiple files (e.g. Claude Code's credentials file plus OAuth account metadata), where a partial write would leave the tool in an inconsistent state.

The same guarantee now applies to context activation across multiple tools. A failed Codex or Gemini apply rolls back any Claude writes that already happened during the same `context use`.

## Credential storage backends

`aisw` supports two credential backends per profile:

**File**  -  credentials are stored as `0600` files under `~/.aisw/profiles/<tool>/<name>/`. This works on all platforms and requires no external dependencies.

**System keyring**  -  credentials are stored in the OS native secure store. The file entry under `~/.aisw/profiles/` still exists but contains only a reference; the sensitive bytes live in the keyring.

Backend selection is automatic based on what the upstream tool is using and what is available on the current machine. On macOS, profiles are typically stored as files in `~/.aisw/` even when the live tool uses the Keychain, because the Keychain entry is written directly during `aisw use`.

### OS keyring support

| Platform | Backend |
|---|---|
| macOS | macOS Keychain via `security-framework` |
| Linux | Secret Service (D-Bus) via `keyring` crate with vendored libdbus |
| Windows | Windows Credential Manager via `keyring` crate |

On Linux, if the Secret Service daemon is not available at runtime (e.g. headless servers), `aisw` falls back to file-backed storage and reports a diagnostic. It will not silently use an insecure path without notifying you.

## Per-tool implementation

### Claude Code

**Credential locations:**
- macOS: `~/Library/Application Support/Claude/` (Keychain) and `~/.claude/.credentials.json` (file fallback)
- Linux/Windows: `~/.claude/.credentials.json`
- OAuth account metadata: `~/.claude.json` (`oauthAccount` field)

**How `aisw` captures credentials:**
- `--api-key`: stores the key directly.
- `--from-live`: reads the current live credentials from file or Keychain.
- Interactive OAuth: spawns `claude auth login`. When Claude's install supports profile-owned auth, `aisw` points login at the profile `CLAUDE_CONFIG_DIR`; otherwise it polls Claude's live credential file and Keychain for changes and captures the result there.

**How `aisw use` applies credentials:**
- Detects whether the live tool is reading from file or Keychain.
- Writes the full credential payload to the appropriate location.
- Updates the `oauthAccount` field in `~/.claude.json` if the profile includes OAuth account metadata.
- With `--state-mode isolated`: sets `CLAUDE_CONFIG_DIR` to the profile directory so Claude reads config, history, and extensions from a profile-specific location.
- With `--state-mode shared`: unsets `CLAUDE_CONFIG_DIR` so Claude reads its standard config directory.

**Important Claude limitation:** when Claude OAuth is backed by the legacy shared live Keychain entry, `CLAUDE_CONFIG_DIR` does not isolate the actual OAuth credential owner. `aisw` blocks isolated mode for that case before mutating live state and points the user to shared mode or API key / token-based alternatives. When Claude scopes auth by `CLAUDE_CONFIG_DIR`, isolated mode remains the durable path. This is an upstream storage limitation, not `aisw` corruption.

**MCP OAuth tokens:** The full credentials payload including `mcpOAuth` keys is preserved when writing to the Keychain. No subset-stripping is performed.

### Codex CLI

**Credential locations:**
- File-backed: `~/.codex/auth.json`
- Keyring-backed: OS credential store under the account identifier Codex uses
- Config: `~/.codex/config.toml`

**How `aisw` captures credentials:**
- `--api-key` / `--from-env`: stores the key directly.
- `--from-live`: reads `auth.json` or queries the live keyring entry using the account identifier Codex writes there. For ChatGPT-managed auth this is treated as a bootstrap import.
- Interactive OAuth: sets `CODEX_HOME` to the profile directory and spawns `codex` so the native device-auth flow writes directly into the profile. This is the durable ChatGPT-managed Codex path because refreshes remain tied to that profile-owned state.

**How `aisw use` applies credentials:**
- Sets `CODEX_HOME` to the profile directory in isolated mode.
- For API-key profiles, shared mode can unset `CODEX_HOME` and keep the standard Codex directory.
- For ChatGPT-managed auth, shared mode is intentionally blocked because Codex refreshes that session in place and the resulting refresh/session state is not safely shareable across multiple live owners.
- For keyring-backed profiles, writes the profile credentials into the keyring account that Codex expects to find.

**State mode:** `CODEX_HOME` overrides where Codex reads its entire config and auth state. Isolated mode gives each profile a fully separate Codex environment and is the only supported durable mode for ChatGPT-managed Codex auth.

### Gemini CLI

**Credential locations:**
- OAuth: `~/.gemini/oauth_creds.json` (primary) and other files under `~/.gemini/`
- API key: `~/.gemini/.env` (`GEMINI_API_KEY=...`)
- Settings: `~/.gemini/settings.json`

**How `aisw` captures credentials:**
- `--api-key` / `--from-env`: stores the key in a profile `.env` file.
- `--from-live`: copies everything under `~/.gemini/` into the profile directory.

### Antigravity CLI

- Live auth: shared OS-native keyring entry documented by upstream behavior
- Live state: `~/.gemini/antigravity-cli/` and `~/.gemini/config/`
- `--from-live`: captures the current live keyring-backed session plus both documented config roots.
- Interactive OAuth: launches `agy`, captures the resulting live keyring/config state, and restores the prior live state unless `--set-active` is requested.
- `use`: restores the managed keyring secret into Antigravity's live keyring entry, then transactionally syncs the documented config roots.

**Important Antigravity limitation:** upstream does not currently document an isolated per-profile auth/data root or profile selector. `aisw` therefore supports Antigravity through shared live switching rather than profile-owned isolated auth. This is a product limitation upstream, not `aisw` corruption.
- Interactive OAuth: sets `GEMINI_CLI_HOME` to a temporary scratch directory, spawns `gemini` so it writes its OAuth cache there, then copies all resulting files from `<scratch>/.gemini/` into the profile directory. The scratch directory is always cleaned up, regardless of success or failure.

  `GEMINI_CLI_HOME` was introduced in Gemini CLI to override the home directory used for config storage. It is cleaner than overriding `HOME` because it does not affect other processes or macOS Keychain lookups that depend on the real home directory.

**How `aisw use` applies credentials:**
- Copies all profile files into `~/.gemini/`, replacing whatever is currently there.
- There is no configurable shared mode because Gemini's auth and broader local state are tightly coupled under `~/.gemini/`. Separating them would risk corrupting the tool's session state.

**State mode:** Gemini is always `isolated`. Each profile carries its own complete `~/.gemini/` state.

## Automatic Synchronization

To handle the fact that tools frequently refresh OAuth tokens in the background, `aisw` implements an automatic synchronization mechanism during profile switching.

When you run `aisw use <tool> <new-profile>`, `aisw` performs a pre-switch check:
1. It identifies the account currently active in the live tool using canonical identity logic.
2. It compares this identity with the identity stored in the *currently active* `aisw` profile.
3. If they match, `aisw` captures the latest live credentials and metadata into the stored profile before switching away.

This ensures that your profiles stay fresh without manual intervention. Synchronization is an observational-only check during `aisw status`, and a mutational operation only during `aisw use`, aligning with the principle that `status` should be side-effect free.

## Identity deduplication and matching

When OAuth credentials are captured or synchronized, `aisw` extracts the authenticated account identity using a unified canonical logic. This logic:
- Decodes JWT payloads (Codex and Gemini) to find `email` or `sub` claims.
- Parses nested metadata (Claude) to find `emailAddress`.
- Normalizes identifiers (lowercase, trim) and handles subject fallbacks.
- Recursively searches JSON structures to find identity fields in varied schemas.

If you attempt to add a second profile for an account that is already stored under a different name, `aisw` rejects it. During synchronization, this same logic ensures that tokens are only updated if they belong to the same account.

## Token expiry warnings

`aisw status` checks the expiry of stored OAuth credentials and warns when:
- A token is already expired.
- A token expires within 24 hours.

The check is informational. `aisw` does not attempt to refresh tokens; that is the responsibility of the upstream tool.

## Config locking

Commands that write to `~/.aisw/config.json` take an exclusive file lock. If two `aisw` commands run concurrently, the second waits briefly and then fails with a clear error rather than writing partial state. This is safe in CI environments where parallel steps might both invoke `aisw`.

Context activation still derives active-context status from current per-tool active profiles. `aisw` does not store a sticky “last selected context” field, because that would become misleading after a manual single-tool switch.

## Backup behavior

Before any destructive operation (remove, rename), `aisw` creates a timestamped backup under `~/.aisw/backups/`. The backup includes profile files and the config snapshot. Backups are listed with `aisw backup list` and restored with `aisw backup restore <id>`.

Automatic backups are also created before profile switching when `backup_on_switch` is true in config (the default). The maximum number of retained backups is controlled by `max_backups` (default: 10); older backups are pruned when the limit is exceeded.

## What aisw does not do

- Does not proxy API traffic. Requests go directly from the tool to the provider.
- Does not inspect or log prompt content.
- Does not transmit credentials or usage data to any remote service.
- Does not manage tool installation, configuration, or settings beyond auth state.
- Does not refresh expired OAuth tokens. Run the provider's own re-auth flow and recapture.
