---
title: Supported Tools
description: See which tools aisw supports and how authentication works for each one.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/supported-tools.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, Supported Tools, reference, does aisw support Claude Code, does aisw support Codex CLI, does aisw support Gemini CLI
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Supported Tools","headline":"Supported Tools","description":"See which tools aisw supports and how authentication works for each one.","url":"https://burakdede.github.io/aisw/supported-tools/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, does aisw support Claude Code, does aisw support Codex CLI, does aisw support Gemini CLI","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.0","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Supported Tools","item":"https://burakdede.github.io/aisw/supported-tools/"}]}]}
---

`aisw` supports the main AI coding CLI tools people usually want to manage across multiple accounts: Claude Code, Codex CLI, and Gemini CLI.

If you are looking for a Claude Code account switcher, a Codex CLI profile manager, or a Gemini CLI multi-account workflow, these are the tools currently covered.

| Tool | Binary expected on PATH | Minimum version known to work |
|---|---|---|
| Claude Code | `claude` | 1.0.0 |
| Codex CLI | `codex` | 1.0.0 |
| Gemini CLI | `gemini` | 0.1.0 |

aisw detects each tool by searching PATH for the binary name. It does not hardcode install locations. If a binary is not found, `aisw status` reports it as not installed and `aisw use` will refuse to switch to that tool.

Version detection runs `<binary> --version` and captures the output as-is. If the binary exits non-zero or produces no output, the version is reported as unknown — this does not prevent aisw from managing the tool's profiles.

## State mode support

Claude Code and Codex CLI support configurable switch behavior:
- `isolated`: switch account credentials and local tool state together
- `shared`: keep the tool's local state shared and switch account credentials only

Gemini CLI is currently isolated-only.

Why Gemini differs:
- Gemini stores credentials and broader local state together under `~/.gemini`
- that native directory can include history, trusted folders, project mappings, settings, and MCP-related config
- a Gemini "shared" mode would therefore share the whole native Gemini state, not just auth

Because of that, `aisw` does not currently expose `--state-mode` for Gemini.

## Typical search intents this page answers

- Which AI CLI tools does aisw support?
- Does aisw support Claude Code?
- Does aisw support OpenAI Codex CLI?
- Does aisw support Google Gemini CLI?
- Can I manage multiple accounts for Claude, Codex, and Gemini from one CLI?
