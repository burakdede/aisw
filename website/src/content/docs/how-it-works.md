---
title: How It Works
description: Profile model, atomic credential switching, OS keyring integration, and per-tool implementation details for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/how-it-works.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, how it works, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"How It Works","headline":"How It Works","description":"Profile model, atomic credential switching, OS keyring integration, and per-tool implementation details for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI.","url":"https://burakdede.github.io/aisw/how-it-works/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, how it works, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.10","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"How It Works","item":"https://burakdede.github.io/aisw/how-it-works/"}]}]}
---

This page explains the design decisions behind `aisw`, how credentials are stored and applied, and the per-tool implementation details for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI.

## The runtime model

There are three distinct kinds of state. Keeping them separate explains most of aisw's behavior:

| State | Where it lives | What it means |
| --- | --- | --- |
| Managed profile | `~/.aisw/profiles/<tool>/<name>/` or the OS keyring | A saved credential snapshot that aisw can apply later |
| Live tool state | The upstream tool's normal files, environment, or keyring entries | What a newly started upstream CLI will read now |
| Registry state | `~/.aisw/config.json` | Which profiles exist, which profile is recorded active, and how contexts map tools to profiles |

`aisw use` synchronizes managed profile state into live tool state and then records the active profile in the registry. A context is only a mapping in the registry; it does not contain another copy of the credentials. The shell hook adds environment exports and workspace checks around that model, but it does not replace it.

This distinction also explains two common surprises:

- Restoring a backup repairs managed profile data; it does not activate that profile. Run `aisw use` explicitly afterward.
- A manual login or an upstream token refresh can change live state without changing aisw's registry. Run `aisw status` or `aisw verify` to detect that drift.

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

The activation sequence is:

1. Resolve the requested profile or context and validate every referenced profile.
2. Acquire the operation lock so another mutating command cannot interleave its writes.
3. Snapshot the affected live files, keyring entries, and active-profile metadata.
4. Apply the target credential and configuration state in a deterministic order.
5. Commit the registry's active-profile metadata only after all live writes succeed.
6. On failure, restore the snapshot, preserve the original failure, and report any rollback or cleanup failure alongside it.

The transaction covers the locations aisw knows how to manage. It cannot roll back an independent process that changes the same upstream files after the snapshot, which is why concurrent mutation is serialized where possible and a fresh agent process is recommended after activation.

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
- Interactive OAuth: spawns `claude auth login`. When Claude's install supports profile-owned auth, `aisw` points login at the profile `CLAUDE_CONFIG_DIR`, waits for a non-empty credential payload, and reads account metadata from that directory; otherwise it polls Claude's live credential file and Keychain for changes and captures the result there.

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
- `--from-live`: copies the live Gemini regular-file tree under `~/.gemini/` into the profile directory.
- Interactive OAuth: sets `GEMINI_CLI_HOME` to a temporary scratch directory, spawns `gemini` so it writes its OAuth cache there, then copies all resulting regular files from `<scratch>/.gemini/` into the profile directory. The scratch directory is always cleaned up, regardless of success or failure.

  `GEMINI_CLI_HOME` was introduced in Gemini CLI to override the home directory used for config storage. It is cleaner than overriding `HOME` because it does not affect other processes or macOS Keychain lookups that depend on the real home directory.

**How `aisw use` applies credentials:**
- Restores the managed Gemini regular-file tree into `~/.gemini/` and removes stale live files from the previously active Gemini profile.
- There is no configurable shared mode because Gemini's auth and broader local state are tightly coupled under `~/.gemini/`. Separating them would risk corrupting the tool's session state.

**State mode:** Gemini is always `isolated`. Each profile carries its own complete `~/.gemini/` state.

### Antigravity CLI

- Live auth: shared OS-native keyring entry documented by upstream behavior
- Live state: `~/.gemini/antigravity-cli/` and `~/.gemini/config/`
- `--from-live`: captures the current live keyring-backed session plus both documented config roots.
- Interactive OAuth: launches `agy`, captures the resulting live keyring/config state, and restores the prior live state unless `--set-active` is requested.
- `use`: restores the managed keyring secret into Antigravity's live keyring entry, then transactionally syncs the documented config roots.

**Important Antigravity limitation:** upstream does not currently document an isolated per-profile auth/data root or profile selector. `aisw` therefore supports Antigravity through shared live switching rather than profile-owned isolated auth. This is a product limitation upstream, not `aisw` corruption.

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
