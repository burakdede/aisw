---
title: Security
description: aisw security posture  -  local-only credential storage, OS keyring integration, file permissions, no remote transmission, and safe switching design.
---

# Security

`aisw` manages authentication credentials for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI. This page documents the security model, storage design, and the boundaries of what `aisw` does and does not do with those credentials.

## Summary

- Credentials are stored locally only  -  no remote service, no telemetry, no sync.
- Sensitive files are written with `0600` permissions (owner read/write only).
- OS keyring integration uses the platform-native API (macOS Keychain, Linux Secret Service, Windows Credential Manager).
- Switching is transactional: a failed write rolls back to the previous state.
- Backups are created before destructive operations.
- `aisw` never reads, logs, or transmits the content of your prompts, conversations, or API responses.

## Credential storage

### What is stored and where

Credentials are stored under `~/.aisw/profiles/<tool>/<name>/`. The central config file `~/.aisw/config.json` contains only profile metadata (name, auth method, timestamps, labels). It does not contain credential material.

The shell installer refuses to write through a symlinked install directory or
binary destination. This prevents a stale `aisw` link from redirecting an
upgrade outside the directory selected by `AISW_INSTALL_DIR`.

For keyring-backed profiles, the sensitive credential bytes are stored in the OS keyring. The profile directory on disk contains a minimal reference or empty file; the actual secret lives in the keyring.

### File permissions

All files written to `~/.aisw/profiles/` are created with `0600` permissions: readable and writable only by the owning user. This applies to API keys, OAuth tokens, and any captured tool state files.

`aisw status` reports a warning if any credential file under `~/.aisw/` has permissions broader than `0600`.

Directories under `~/.aisw/` are created with `0700`.

### OS keyring integration

`aisw` uses the platform-native keyring API through the `keyring` crate:

| Platform | Backend |
|---|---|
| macOS | macOS Keychain via `security-framework`, with app-path ACL limiting access to the `claude` binary |
| Linux | Secret Service protocol (GNOME Keyring, KWallet) with vendored libdbus  -  no system dbus development package required |
| Windows | Windows Credential Manager via WinCred API |

The repository also runs a real credential-store canary across the supported
macOS, Linux, and Windows runners. It creates uniquely named temporary entries,
exercises profile switching and removal, and restores any pre-existing entry it
touches. The canary runs weekly or can be started manually from GitHub Actions;
it has a ten-minute per-platform timeout, and it is intentionally fail-closed
when a runner cannot initialize its native credential store.

On macOS, when writing Claude Code credentials to the Keychain, `aisw` sets a trusted-application ACL so the entry is bound to the `claude` binary path. This prevents other applications from reading the credential without a Keychain access prompt.

On Linux, if the Secret Service daemon is not running (common on headless servers), `aisw` detects this at runtime, emits a diagnostic, and falls back to `0600` file storage rather than silently using a less-secure path.

### No remote transmission

`aisw` is a local tool. It does not:

- Send credentials to any server.
- Call any `aisw`-operated API.
- Include telemetry, analytics, or crash reporting.
- Connect to the network for any purpose.

All operations are local filesystem and OS keyring operations. You can audit this by inspecting the source at [github.com/burakdede/aisw](https://github.com/burakdede/aisw).

## Switching safety

### Transactional writes

Profile activation uses a snapshot-and-apply model. Before writing any live credential file, the current live state is captured. If any file write fails partway through, the snapshot is restored atomically. You never end up with a partially applied profile.

If the commit and its automatic rollback both fail, `aisw` reports both errors
so the operator knows that live state needs inspection before retrying. Staged
file cleanup failures are reported alongside the operation error as well.

This is particularly important for Claude Code, which stores credentials across multiple locations (the credentials file and OAuth account metadata in `~/.claude.json`). A failed write to either location triggers a full rollback.

### Backups before destructive operations

Before any remove or rename operation, `aisw` creates a timestamped backup under `~/.aisw/backups/`. Backups can be listed and restored:

```sh
aisw backup list
aisw backup restore <backup_id> --yes
```

Restore snapshots the affected profile files, secure credentials, and config
metadata before applying a backup. If a later entry fails, it restores every
affected entry and reports any recovery failure instead of leaving file,
keyring, and config state silently divergent.

Backups are also created before profile switching when `backup_on_switch` is enabled in config (the default).

### Config locking

All commands that modify `~/.aisw/config.json` take an exclusive lock on the file before writing. If two `aisw` commands run concurrently, the second will wait briefly and then fail with a clear error rather than producing a partial write. This prevents config corruption in parallel CI environments.

Initialization, live profile switches, backup restores, and profile lifecycle
mutations take the same operation lock for their storage changes. This
prevents concurrent `init`, `use`, `context use`, `backup restore`, `rename`,
and `remove` commands from interleaving credential-file, keyring, shell-hook,
and active-profile metadata updates. Machine-readable initialization holds
the lock while creating or loading AISW state, then performs read-only
detection outside it.

Profile additions and live imports use the same lock because OAuth capture and
credential writes can also change the live credential owner. An interactive
login may therefore make a concurrent switch wait or fail with the normal lock
timeout rather than allowing two commands to compete for live state.

### Input validation

Profile names are restricted to `a-z`, `A-Z`, `0-9`, `-`, and `_`, and every read and write resolves inside the profile directory, so a name can never reach a path outside `~/.aisw/profiles/`. `aisw` refuses to read or write through a symlink at a credential path. Permission repair also refuses a symlinked `AISW_HOME` root, so it cannot traverse an alias into an unrelated directory.

API keys must be a single line. Keys containing control characters are rejected, because stored credentials are later materialized into formats where such characters change meaning rather than being escaped  -  a Gemini API key is written to a `.env` file the CLI sources, so an embedded newline would otherwise inject additional environment variables.

### Deletion

`aisw uninstall --remove-data` deletes `~/.aisw/` **and** the OS keyring entries `aisw` created for profiles and backups, so no managed secret outlives the data directory. It refuses to run when `AISW_HOME` points at your home directory. If managed metadata cannot be read or a keyring entry cannot be removed, uninstall fails before deleting `AISW_HOME`, leaving the data available for retry.

## OAuth flows

During interactive OAuth, `aisw` spawns the upstream tool's native auth binary (`claude auth login`, `codex`, `gemini`, or `agy`) and waits for credentials to appear in the expected locations. It does not intercept or proxy the authentication request. The token is issued directly by the provider to the tool. If the capture fails or times out, the spawned login process is terminated rather than left running.

For Gemini, `aisw` sets `GEMINI_CLI_HOME` to a temporary scratch directory so the OAuth cache is written there rather than to `~/.gemini/`. This prevents the OAuth flow from polluting the live account. The scratch directory is deleted after the flow completes, regardless of whether it succeeds or fails.

For Claude Code, `aisw` uses the most reliable auth target the installed Claude build supports. When Claude scopes auth by `CLAUDE_CONFIG_DIR`, `aisw` runs OAuth capture inside the profile-owned directory. When Claude still uses a shared live Keychain credential, `aisw` leaves login pointed at Claude's live state and detects completion by polling the live credential file and OS keychain for changes.

## Scope of access

`aisw` reads and writes only:

1. Files under `~/.aisw/` (profiles, backups, config).
2. The tool's live credential locations (`~/.claude/`, `~/.codex/`, `~/.gemini/`  -  which also holds Antigravity's config roots  -  and their respective keychain entries).
3. Shell config files (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/config.fish`)  -  only when you explicitly run `aisw shell-hook` and redirect its output there yourself, or when `aisw uninstall` removes managed hook blocks you previously added.

It does not access any other files or system resources.

For both managed profiles and live agent state, `aisw` refuses to traverse a
symlinked directory or intermediate parent. This keeps credential reads and
writes inside the documented storage boundary; symlink-based layouts must be
replaced with real directories before switching or restoring state.

## Reporting a vulnerability

To report a security issue, open a private advisory at [github.com/burakdede/aisw/security/advisories](https://github.com/burakdede/aisw/security/advisories) or email the repository owner directly.

Do not open a public GitHub issue for security vulnerabilities.
