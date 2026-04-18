---
title: Supported Tools
description: Tool support matrix and auth/backend behavior.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/supported-tools.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, supported tools, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Supported Tools","headline":"Supported Tools","description":"Tool support matrix and auth/backend behavior.","url":"https://burakdede.github.io/aisw/supported-tools/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, supported tools, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Supported Tools","item":"https://burakdede.github.io/aisw/supported-tools/"}]}]}
---

`aisw` currently supports:

- Claude Code (`claude`)
- Codex CLI (`codex`)
- Gemini CLI (`gemini`)

## Binary detection

`aisw` resolves tools from `PATH` and checks version via `<binary> --version`.

| Tool | Binary | Minimum known-good version |
|---|---|---|
| Claude Code | `claude` | `1.0.0` |
| Codex CLI | `codex` | `1.0.0` |
| Gemini CLI | `gemini` | `0.1.0` |

If binary lookup fails, `aisw status` reports the tool as missing and `aisw use` for that tool is blocked.

## State mode support

| Tool | `--state-mode isolated` | `--state-mode shared` |
|---|---|---|
| Claude Code | supported | supported |
| Codex CLI | supported | supported |
| Gemini CLI | supported | not supported |

Gemini remains isolated-only because auth and broader CLI state are coupled under `~/.gemini`.

## Auth backend support

| Tool | Backend | Import | Use | Notes |
|---|---|---|---|---|
| Claude Code | file credentials | yes | yes | profile stores file state |
| Claude Code | system keyring | yes (when live entry can be read) | yes | profile can remain keyring-backed |
| Codex CLI | file `auth.json` | yes | yes | portable across OSes |
| Codex CLI | system keyring (discoverable account) | yes | yes | uses resolved keyring account |
| Codex CLI | system keyring (not discoverable) | partial | fail-closed | no fabricated keyring account |
| Gemini CLI | file-backed `~/.gemini` state | yes | yes | no keyring mode in `aisw` |

## References

- [Auth Storage Matrix](https://github.com/burakdede/aisw/blob/main/AUTH_STORAGE_MATRIX.md)
- [Acceptance Matrix](https://github.com/burakdede/aisw/blob/main/docs/acceptance-matrix.md)
