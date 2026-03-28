---
title: aisw AI Switcher Documentation
description: Install, configure, and use aisw, the AI Switcher for Claude Code, Codex CLI, and Gemini CLI accounts.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/index.md
template: splash
hero:
  title: "aisw"
  tagline: "AI / Coding Agent account manager and switcher for Claude Code, Codex CLI, and Gemini CLI. Current release: v0.1.1."
  actions:
    - text: Quickstart
      link: /aisw/quickstart/
      variant: primary
    - text: Watch Demo
      link: /aisw/#watch-aisw-work
      variant: secondary
    - text: Releases
      link: https://github.com/burakdede/aisw/releases
      variant: minimal
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, aisw AI Switcher Documentation, overview, AI Switcher, aisw AI Switcher, AI CLI account switcher, AI CLI account manager, AI account manager, coding agent account manager, Claude Code account switcher, Claude Code account manager, Codex CLI account switcher, Codex CLI account manager, Gemini CLI account switcher, Gemini CLI account manager, manage multiple AI CLI accounts
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"WebPage","name":"aisw AI Switcher Documentation","headline":"aisw AI Switcher Documentation","description":"Install, configure, and use aisw, the AI Switcher for Claude Code, Codex CLI, and Gemini CLI accounts.","url":"https://burakdede.github.io/aisw/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, AI Switcher, aisw AI Switcher, AI CLI account switcher, AI CLI account manager, AI account manager, coding agent account manager, Claude Code account switcher, Claude Code account manager, Codex CLI account switcher, Codex CLI account manager, Gemini CLI account switcher, Gemini CLI account manager, manage multiple AI CLI accounts","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.1.1","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"}]},{"@type":"FAQPage","mainEntity":[{"@type":"Question","name":"What does aisw actually change when I switch accounts?","acceptedAnswer":{"@type":"Answer","text":"aisw use applies the selected profile into the live config location that Claude Code, Codex CLI, or Gemini CLI already reads. It does not patch the tool binary, install a proxy, or change anything outside the relevant local credential and config files."}},{"@type":"Question","name":"Does aisw send credentials or prompts over the network?","acceptedAnswer":{"@type":"Answer","text":"No. aisw itself does not proxy requests, inspect prompts, or send your credentials to a remote service. It is a local credential and profile switcher."}},{"@type":"Question","name":"Where are profiles stored, and how are they protected?","acceptedAnswer":{"@type":"Answer","text":"Stored profiles live under ~/.aisw/profiles/<tool>/<name>/. Credential files are written with 0600 permissions so only your user can read or write them, and aisw status reports files that are broader than that."}}]}]}
---

`aisw` stands for AI Switcher. It is a multi-account manager and account switcher for Claude Code, Codex CLI, and Gemini CLI, built to help you switch AI CLI accounts without manually copying credential files, editing config directories, or re-running login flows every time you hit a usage limit.

## Install

### Shell installer

```sh
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh
```

### Cargo

```sh
cargo install aisw
```

## Watch aisw work

<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/asciinema-player@3.8.0/dist/bundle/asciinema-player.css">
<div id="aisw-home-demo"></div>
<script src="https://cdn.jsdelivr.net/npm/asciinema-player@3.8.0/dist/bundle/asciinema-player.min.js"></script>
<script>
  AsciinemaPlayer.create('/aisw/demos/aisw-important-workflows.cast', document.getElementById('aisw-home-demo'), {
    cols: 108,
    rows: 32,
    autoPlay: false,
    loop: false,
    preload: true,
    fit: 'width',
    poster: 'npt:2',
    terminalFontSize: '16px',
    markers: [
      [2.4, 'Init'],
      [9.6, 'Add work'],
      [23.4, 'Add personal'],
      [37.9, 'Switch'],
      [45.7, 'Status'],
      [52.6, 'Rename'],
      [61.2, 'List'],
      [67.9, 'Remove'],
      [76.7, 'Backups'],
      [83.9, 'Restore']
    ]
  });
</script>

## What aisw helps with

Developers usually find `aisw` when they are trying to solve one of these problems:

- switch between multiple Claude Code accounts
- switch between multiple Codex CLI accounts
- switch between multiple Gemini CLI accounts
- manage several AI CLI subscriptions on one machine
- rotate between work and personal AI coding tool profiles
- keep Claude, Codex, and Gemini credentials organized without manual file copying

If you were searching for an AI CLI account switcher, a multi-account CLI manager, or a way to manage multiple Claude, Codex, or Gemini logins locally, this documentation is the right place to start.

## Start here

| Document | Description |
|---|---|
| [Why aisw?](/aisw/why-aisw/) | Why you need an AI agent account manager |
| [Quickstart](/aisw/quickstart/) | Install aisw, run first-time setup, and switch accounts quickly |
| [Commands](/aisw/commands/) | Full reference for all subcommands and flags |
| [Adding Profiles](/aisw/adding-profiles/) | OAuth and API key auth flows per tool |
| [Automation and Scripting](/aisw/automation/) | Prompt behavior, JSON output, stdout/stderr expectations, and scripting patterns |

## Value at a glance

- **Zero Manual File Copying:** Switch profiles with one command. No more searching for hidden `.env` or `.credentials.json` files.
- **Safety First:** Automatic backups before every switch and enforced `0600` permissions.
- **Identity Awareness:** Prevent duplicate aliases by automatically resolving account emails and IDs.
- **Seamless Setup:** `aisw init` imports your existing credentials in seconds.

## Setup and operation

| Document | Description |
|---|---|
| [Shell Integration](/aisw/shell-integration/) | Shell hook setup for bash, zsh, fish |
| [Supported Tools](/aisw/supported-tools/) | Tool compatibility, binary names, auth methods |
| [Configuration](/aisw/configuration/) | `~/.aisw/config.json` schema and settings |

## Common questions

- [Troubleshooting](/aisw/troubleshooting/): Issues with shell hooks, tool detection, or permissions.
- [What aisw actually changes when I switch accounts?](#what-aisw-actually-changes-when-i-switch-accounts)
- [Does aisw send credentials or prompts over the network?](#does-aisw-send-credentials-or-prompts-over-the-network)
- [Where are profiles stored, and how are they protected?](#where-are-profiles-stored-and-how-are-they-protected)

### What does aisw actually change when I switch accounts?

`aisw use` applies the selected profile into the live config location that Claude Code, Codex CLI, or Gemini CLI already reads. It does not patch the tool binary, install a proxy, or change anything outside the relevant local credential and config files.

### Does aisw send credentials or prompts over the network?

No. `aisw` itself does not proxy requests, inspect prompts, or send your credentials to a remote service. It is a local credential and profile switcher.

### Where are profiles stored, and how are they protected?

Stored profiles live under `~/.aisw/profiles/<tool>/<name>/`. Credential files are written with `0600` permissions so only your user can read or write them, and `aisw status` reports files that are broader than that.

### Can I use this for work, personal, and backup accounts across different tools?

Yes. A common setup is separate work, personal, client, or backup profiles for Claude Code, Codex CLI, and Gemini CLI so you can switch in seconds when a quota runs out or you need a different subscription.
