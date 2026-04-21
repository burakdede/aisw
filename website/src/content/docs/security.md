---
title: Security
description: How aisw stores and protects credentials — local-only storage, OS keyring integration, file permissions, transactional writes, and OAuth flow safety.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/security.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, security, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Security","headline":"Security","description":"How aisw stores and protects credentials — local-only storage, OS keyring integration, file permissions, transactional writes, and OAuth flow safety.","url":"https://burakdede.github.io/aisw/security/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, security, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Security","item":"https://burakdede.github.io/aisw/security/"}]}]}
---

`aisw` manages authentication credentials for Claude Code, Codex CLI, and Gemini CLI. This page documents the security model, storage design, and the boundaries of what `aisw` does and does not do with those credentials.

## Summary

- Credentials are stored locally only — no remote service, no telemetry, no sync.
- Sensitive files are written with `0600` permissions (owner read/write only).
- OS keyring integration uses the platform-native API (macOS Keychain, Linux Secret Service, Windows Credential Manager).
- Switching is transactional: a failed write rolls back to the previous state.
- Backups are created before destructive operations.
- `aisw` never reads, logs, or transmits the content of your prompts, conversations, or API responses.

## Credential storage

### What is stored and where

Credentials are stored under `~/.aisw/profiles/<tool>/<name>/`. The central config file `~/.aisw/config.json` contains only profile metadata (name, auth method, timestamps, labels). It does not contain credential material.

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
| Linux | Secret Service protocol (GNOME Keyring, KWallet) with vendored libdbus — no system dbus development package required |
| Windows | Windows Credential Manager via WinCred API |

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

This is particularly important for Claude Code, which stores credentials across multiple locations (the credentials file and OAuth account metadata in `~/.claude.json`). A failed write to either location triggers a full rollback.

### Backups before destructive operations

Before any remove or rename operation, `aisw` creates a timestamped backup under `~/.aisw/backups/`. Backups can be listed and restored:

```sh
aisw backup list
aisw backup restore <backup_id> --yes
```

Backups are also created before profile switching when `backup_on_switch` is enabled in config (the default).

### Config locking

All commands that modify `~/.aisw/config.json` take an exclusive lock on the file before writing. If two `aisw` commands run concurrently, the second will wait briefly and then fail with a clear error rather than producing a partial write. This prevents config corruption in parallel CI environments.

## OAuth flows

During interactive OAuth, `aisw` spawns the upstream tool's native auth binary (`claude auth login`, `codex`, or `gemini`) and waits for credentials to appear in the expected locations. It does not intercept or proxy the authentication request. The token is issued directly by the provider to the tool.

For Gemini, `aisw` sets `GEMINI_CLI_HOME` to a temporary scratch directory so the OAuth cache is written there rather than to `~/.gemini/`. This prevents the OAuth flow from polluting the live account. The scratch directory is deleted after the flow completes, regardless of whether it succeeds or fails.

For Claude Code, `aisw` intentionally does not set `CLAUDE_CONFIG_DIR` during OAuth capture. This allows Claude to run its native authentication flow without fallback-induced changes in behavior. `aisw` detects completion by polling the live credential file and OS keychain for changes.

## Scope of access

`aisw` reads and writes only:

1. Files under `~/.aisw/` (profiles, backups, config).
2. The tool's live credential locations (`~/.claude/`, `~/.codex/`, `~/.gemini/`, and their respective keychain entries).
3. Shell config files (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/config.fish`) — only when you explicitly run `aisw shell-hook` and redirect its output there yourself, or when `aisw uninstall` removes managed hook blocks you previously added.

It does not access any other files or system resources.

## Reporting a vulnerability

To report a security issue, open a private advisory at [github.com/burakdede/aisw/security/advisories](https://github.com/burakdede/aisw/security/advisories) or email the repository owner directly.

Do not open a public GitHub issue for security vulnerabilities.
