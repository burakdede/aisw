---
title: Adding Profiles
description: Add profiles with API keys or interactive OAuth flows.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/adding-profiles.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, adding profiles, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Adding Profiles","headline":"Adding Profiles","description":"Add profiles with API keys or interactive OAuth flows.","url":"https://burakdede.github.io/aisw/adding-profiles/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, adding profiles, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Adding Profiles","item":"https://burakdede.github.io/aisw/adding-profiles/"}]}]}
---

Use `aisw add <tool> <profile>` to create named profiles for Claude Code, Codex CLI, or Gemini CLI.

## Quick syntax

```text
aisw add claude <profile> [--api-key KEY] [--label TEXT] [--set-active]
aisw add codex <profile> [--api-key KEY] [--from-env] [--label TEXT] [--set-active]
aisw add gemini <profile> [--api-key KEY] [--from-env] [--label TEXT] [--set-active]
```

Without `--api-key` or `--from-env`, `aisw add` runs interactive auth.

## API-key flows

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"
```

`--from-env` reads tool-specific env vars:

- Claude: `ANTHROPIC_API_KEY`
- Codex: `OPENAI_API_KEY`
- Gemini: `GEMINI_API_KEY`

Example:

```sh
aisw add codex ci --from-env
```

## OAuth flows

```sh
aisw add claude personal
aisw add codex personal
aisw add gemini personal
```

- Claude: runs the vendor auth login flow in the profile context.
- Codex: runs device-auth login in the profile context.
- Gemini: runs interactive login in an isolated HOME, then captures resulting `~/.gemini` auth state.

## Useful flags

- `--label "..."`: human-readable label
- `--set-active`: switch to the profile immediately after add

Example:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY" --label "Work" --set-active
```

## Where profile data is stored

```text
~/.aisw/profiles/<tool>/<profile>/
```

Credential files are written with `0600` permissions.

## Duplicate-account behavior

When identity can be resolved from stored auth data, `aisw` prevents creating multiple profile names for the same underlying account.

## Related

- [Quickstart](/aisw/quickstart/)
- [Commands](/aisw/commands/)
- [Supported Tools](/aisw/supported-tools/)
