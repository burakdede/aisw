---
title: aisw documentation
description: aisw manages named profiles and contexts for Claude Code, Codex CLI, and Gemini CLI. Switch work, personal, and client accounts, then keep the right coding agent profile active per repo.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/index.md
template: splash
hero:
  title: "aisw"
  tagline: "Account manager and switcher for Claude Code, Codex CLI, and Gemini CLI. Current release: v0.3.8."
  actions:
    - text: Quickstart
      link: /aisw/quickstart/
      variant: primary
    - text: Commands
      link: /aisw/commands/
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
      content: aisw, claude code, codex cli, gemini cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, aisw documentation, overview
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"WebPage","name":"aisw documentation","headline":"aisw documentation","description":"aisw manages named profiles and contexts for Claude Code, Codex CLI, and Gemini CLI. Switch work, personal, and client accounts, then keep the right coding agent profile active per repo.","url":"https://burakdede.github.io/aisw/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, aisw documentation, overview","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.8","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"}]},{"@type":"FAQPage","mainEntity":[{"@type":"Question","name":"What does aisw actually change when I switch accounts?","acceptedAnswer":{"@type":"Answer","text":"aisw use applies the selected profile into the live config location that Claude Code, Codex CLI, or Gemini CLI already reads. It does not patch the tool binary, install a proxy, or change anything outside the relevant local credential and config files."}},{"@type":"Question","name":"Does aisw send credentials or prompts over the network?","acceptedAnswer":{"@type":"Answer","text":"No. aisw itself does not proxy requests, inspect prompts, or send your credentials to a remote service. It is a local credential and profile switcher."}},{"@type":"Question","name":"Where are profiles stored, and how are they protected?","acceptedAnswer":{"@type":"Answer","text":"Stored profiles live under ~/.aisw/profiles/<tool>/<name>/. Credential files are written with 0600 permissions so only your user can read or write them, and aisw status reports files that are broader than that."}}]}]}
---

Named profile and context manager for Claude Code, Codex CLI, and Gemini CLI. Store per-tool accounts, save mixed-name work modes, and switch between them in one command across all three AI coding agents  -  on macOS, Linux, and Windows.

If you maintain separate work and personal accounts for Claude Code, Codex, or Gemini  -  or manage credentials for multiple clients  -  `aisw` gives you one command to switch instead of manually editing `~/.claude/.credentials.json`, juggling `CLAUDE_CONFIG_DIR` overrides, or copying `auth.json` files between directories. For Codex ChatGPT-managed auth, the durable model is one isolated `CODEX_HOME` per profile, not copied shared session state.

It is built for the questions people actually ask:

- How do I switch between two Claude Code accounts?
- How do I keep separate Codex CLI accounts for different clients?
- How do I store work and personal Gemini CLI profiles on one machine?
- How do I make sure the right coding agent account is active in the right repo?

`aisw` answers those with three primitives:

- Profiles: named saved accounts for one tool
- Contexts: one saved work mode across multiple tools
- Workspace guardrails: repo-aware warnings or blocks before launching the wrong account

## Install

```sh
brew tap burakdede/tap && brew install aisw
```

Other installers:

```sh
# Shell installer (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh

# Cargo
cargo install aisw
```

## First run

```sh
aisw init
```

`init` creates `~/.aisw/`, configures the optional shell hook, and offers to import any currently logged-in tool accounts so you start with zero manual re-authentication.

## Core workflow

```sh
# Store profiles
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal              # launches interactive OAuth
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"

# Activate a profile
aisw use claude work
aisw use --all --profile personal     # switch all tools at once

# Save and activate a mixed-name context
aisw context create acme --claude acme-claude --codex acme-codex --gemini acme-gemini
aisw context use acme

# Inspect state
aisw status
aisw status --context
aisw list
```

## Common situations

### Work and personal accounts for the same tool

Store both once, then switch by name:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal
aisw use claude work
```

### Mixed client setup across Claude, Codex, and Gemini

Use a context when each tool needs a different profile name:

```sh
aisw context create client-acme \
  --claude acme-claude \
  --codex client-a-openai \
  --gemini gemini-consulting

aisw context use client-acme
```

### Wrong-account protection per repo

Use workspace guardrails when the repo itself should enforce the right work mode:

```sh
aisw workspace bind . --context client-acme
aisw workspace guard --mode strict
```

## Start here

1. [Quickstart](/aisw/quickstart/)  -  install, first profile, first switch
2. [Common switching situations](/aisw/common-situations/)  -  work/personal, client, repo guardrails
3. [Commands](/aisw/commands/)  -  complete syntax and flag reference
4. [How it works](/aisw/how-it-works/)  -  design decisions, credential storage, platform behavior
5. [Security](/aisw/security/)  -  local-only storage, keyring integration, file permissions
6. [Automation and scripting](/aisw/automation/)  -  CI patterns, JSON output, non-interactive mode
7. [Troubleshooting](/aisw/troubleshooting/)  -  common failures and diagnostics

## Additional reference

- [Adding profiles](/aisw/adding-profiles/)
- [Shell integration](/aisw/shell-integration/)
- [Workspace guardrails](/aisw/workspace/)
- [Why aisw](/aisw/why-aisw/)
- [Supported tools](/aisw/supported-tools/)
- [Configuration](/aisw/configuration/)
- [Changelog](https://github.com/burakdede/aisw/releases)
