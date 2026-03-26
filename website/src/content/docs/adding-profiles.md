---
title: Adding Profiles
description: Understand OAuth and API key profile flows for each supported tool.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/adding-profiles.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, Adding Profiles, reference, add second Claude Code account, add second Codex CLI account, add second Gemini CLI account, AI CLI OAuth profile manager
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Adding Profiles","headline":"Adding Profiles","description":"Understand OAuth and API key profile flows for each supported tool.","url":"https://burakdede.github.io/aisw/adding-profiles/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, add second Claude Code account, add second Codex CLI account, add second Gemini CLI account, AI CLI OAuth profile manager","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.1.0","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Adding Profiles","item":"https://burakdede.github.io/aisw/adding-profiles/"}]}]}
---

This guide explains how to add and store multiple accounts for Claude Code, Codex CLI, and Gemini CLI.

Use it if you want to:

- add a second Claude Code account
- add a second Codex CLI account
- add a second Gemini CLI account
- keep work and personal AI CLI profiles separate
- understand whether each tool uses OAuth, API keys, or local config files

## Claude Code — API key

```
aisw add claude <name> --api-key <key>
```

Stores `{"apiKey": "<key>"}` in `~/.aisw/profiles/claude/<name>/.credentials.json` with `0600` permissions. When you switch to this profile, aisw copies that credentials file into Claude's live config location.

Anthropic's official docs show API key hints beginning with `sk-ant-api03-...`, but do not appear to publish a strict public format specification for Claude Code API keys.

`aisw` does not currently enforce a Claude-specific prefix or minimum length. It only validates that the key is not empty. The `sk-ant-...` examples in this repo are illustrative, not a claimed official format rule.

Official references:

- https://docs.anthropic.com/en/api/admin-api/apikeys/get-api-key
- https://docs.anthropic.com/en/api/admin-api/apikeys/update-api-key

## Claude Code — OAuth (browser login)

```
aisw add claude <name>
```

Spawns `claude` with `CLAUDE_CONFIG_DIR` set to the profile directory:

```
CLAUDE_CONFIG_DIR=~/.aisw/profiles/claude/<name> claude
```

Claude's OAuth flow opens a browser window. Once you authenticate, Claude writes `.credentials.json` into `CLAUDE_CONFIG_DIR`. aisw polls for this file (every 500ms, up to 120 seconds) and registers the profile once it appears.

If aisw can resolve the authenticated OAuth account identity from the stored credentials, it prevents creating a second profile alias for the same account. If identity cannot be resolved reliably, the add still succeeds with a warning.

**macOS Keychain is never used.** The `CLAUDE_CONFIG_DIR` override causes Claude to store credentials as a plain file instead of in Keychain. This is intentional — it is what makes profiles portable and switchable.

## Codex CLI — API key

```
aisw add codex <name> --api-key <key>
```

Writes two files into the profile directory:

- `config.toml` — sets `cli_auth_credentials_store = "file"` so Codex reads from a file instead of the OS keyring
- `auth.json` — stores `{"token": "<key>"}`

Both files are written with `0600` permissions. When you switch to this profile, aisw copies `auth.json` into `~/.codex/` and ensures Codex is configured to read credentials from a file without overwriting unrelated settings in `config.toml`.

OpenAI's official docs document API key authentication and management, but `aisw` does not currently enforce a Codex key prefix or minimum length. It only validates that the key is not empty. The `sk-...` examples in this repo are illustrative, not a claimed official format rule.

Official references:

- https://platform.openai.com/docs/api-reference/
- https://platform.openai.com/docs/api-reference/project-api-keys

## Codex CLI — OAuth

```
aisw add codex <name>
```

Spawns `codex` with `CODEX_HOME` set to the profile directory (with `config.toml` pre-written). Codex's login flow writes `auth.json` into `CODEX_HOME`. aisw polls for the file and registers the profile on success.

If aisw can resolve the authenticated OAuth account identity from the stored credentials, it prevents creating a second profile alias for the same account. If identity cannot be resolved reliably, the add still succeeds with a warning.

## Gemini CLI — API key

```
aisw add gemini <name> --api-key AIza...
```

Writes `GEMINI_API_KEY=<key>` to `.env` in the profile directory with `0600` permissions. When you switch to this profile, aisw copies this file to `~/.gemini/.env` — no shell eval needed.

Google's official docs document using a Gemini API key via the `GEMINI_API_KEY` environment variable, but `aisw` does not currently enforce a Gemini key prefix or minimum length. It only validates that the key is not empty. The `AIza...` example in this repo is illustrative, not a claimed official format rule.

Official references:

- https://ai.google.dev/gemini-api/docs/quickstart
- https://ai.google.dev/gemini-api/docs/api-key

## Gemini CLI — OAuth

```
aisw add gemini <name>
```

Spawns `gemini` with its config directory set to the profile directory. OAuth token files are written there and copied to the active location on switch.

If aisw can resolve the authenticated OAuth account identity from the stored credentials, it prevents creating a second profile alias for the same account. If identity cannot be resolved reliably, the add still succeeds with a warning.
