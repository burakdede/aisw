---
title: aisw Documentation
description: Install, configure, and use aisw to switch between Claude, Codex, and Gemini CLI accounts.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/index.md
template: splash
hero:
  title: "aisw"
  tagline: "Switch between Claude Code, Codex CLI, and Gemini CLI accounts with one local CLI. Current release: v0.1.0."
  actions:
    - text: Quickstart
      link: /aisw/quickstart/
      variant: primary
    - text: Releases
      link: /aisw/releases/
      variant: secondary
    - text: GitHub
      link: https://github.com/burakdede/aisw
      variant: minimal
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI CLI account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, aisw Documentation, overview, AI CLI account switcher, Claude Code account switcher, Codex CLI account switcher, Gemini CLI account switcher, manage multiple AI CLI accounts
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"WebPage","name":"aisw Documentation","headline":"aisw Documentation","description":"Install, configure, and use aisw to switch between Claude, Codex, and Gemini CLI accounts.","url":"https://burakdede.github.io/aisw/","inLanguage":"en","keywords":"aisw, AI CLI account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, AI CLI account switcher, Claude Code account switcher, Codex CLI account switcher, Gemini CLI account switcher, manage multiple AI CLI accounts","image":"https://burakdede.github.io/aisw/aisw.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.1.0","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"}]},{"@type":"FAQPage","mainEntity":[{"@type":"Question","name":"Can aisw switch between multiple Claude Code accounts?","acceptedAnswer":{"@type":"Answer","text":"Yes. aisw can store and switch multiple Claude Code profiles, including API key and OAuth-based profiles."}},{"@type":"Question","name":"Can aisw manage both Codex CLI and Gemini CLI accounts too?","acceptedAnswer":{"@type":"Answer","text":"Yes. aisw supports Claude Code, Codex CLI, and Gemini CLI in one local profile manager."}},{"@type":"Question","name":"Does aisw proxy requests or inspect prompts?","acceptedAnswer":{"@type":"Answer","text":"No. aisw is a local credential and profile switcher. It does not proxy traffic, inspect prompts, or run a gateway service."}}]}]}
---

> Current documented CLI release: `v0.1.0`. Use [Quickstart](/aisw/quickstart/) to install and start switching profiles.

`aisw` is a multi-account manager and account switcher for Claude Code, Codex CLI, and Gemini CLI. It helps you switch AI CLI accounts without manually copying credential files, editing config directories, or re-running login flows every time you hit a usage limit.

Use this documentation to install the tool, set up profiles, understand how switching works, and operate the release workflow.

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
| [Quickstart](/aisw/quickstart/) | Install aisw, run first-time setup, and switch accounts quickly |
| [Commands](/aisw/commands/) | Full reference for all subcommands and flags |
| [Adding Profiles](/aisw/adding-profiles/) | OAuth and API key auth flows per tool |

## Setup and operation

| Document | Description |
|---|---|
| [Shell Integration](/aisw/shell-integration/) | Shell hook setup for bash, zsh, fish |
| [Supported Tools](/aisw/supported-tools/) | Tool compatibility, binary names, auth methods |
| [Configuration](/aisw/configuration/) | `~/.aisw/config.json` schema and settings |

## Common questions

### Can aisw switch between multiple Claude Code accounts?

Yes. `aisw` can store and switch multiple Claude Code profiles, including API key and OAuth-based profiles.

### Can aisw manage both Codex CLI and Gemini CLI accounts too?

Yes. `aisw` supports Claude Code, Codex CLI, and Gemini CLI in one local profile manager.

### Does aisw proxy requests or inspect prompts?

No. `aisw` is a local credential and profile switcher. It does not proxy traffic, inspect prompts, or run a gateway service.

### Is this useful for work and personal accounts?

Yes. A common use case is keeping separate work, personal, client, or backup AI CLI accounts and switching between them quickly.
