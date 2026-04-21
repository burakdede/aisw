---
title: aisw documentation
description: aisw manages named profiles for Claude Code, Codex CLI, and Gemini CLI. Switch between work, personal, and client accounts with one command on macOS, Linux, and Windows.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/index.md
template: splash
hero:
  title: "aisw"
  tagline: "Account manager and switcher for Claude Code, Codex CLI, and Gemini CLI. Current release: v0.3.2."
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
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, aisw documentation, overview
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"WebPage","name":"aisw documentation","headline":"aisw documentation","description":"aisw manages named profiles for Claude Code, Codex CLI, and Gemini CLI. Switch between work, personal, and client accounts with one command on macOS, Linux, and Windows.","url":"https://burakdede.github.io/aisw/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, aisw documentation, overview","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"}]},{"@type":"FAQPage","mainEntity":[{"@type":"Question","name":"What does aisw actually change when I switch accounts?","acceptedAnswer":{"@type":"Answer","text":"aisw use applies the selected profile into the live config location that Claude Code, Codex CLI, or Gemini CLI already reads. It does not patch the tool binary, install a proxy, or change anything outside the relevant local credential and config files."}},{"@type":"Question","name":"Does aisw send credentials or prompts over the network?","acceptedAnswer":{"@type":"Answer","text":"No. aisw itself does not proxy requests, inspect prompts, or send your credentials to a remote service. It is a local credential and profile switcher."}},{"@type":"Question","name":"Where are profiles stored, and how are they protected?","acceptedAnswer":{"@type":"Answer","text":"Stored profiles live under ~/.aisw/profiles/<tool>/<name>/. Credential files are written with 0600 permissions so only your user can read or write them, and aisw status reports files that are broader than that."}}]}]}
---

Named profile manager for Claude Code, Codex CLI, and Gemini CLI. Store and switch between multiple accounts in one command across all three AI coding agents  -  on macOS, Linux, and Windows.

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

# Inspect state
aisw status
aisw list
```

## Start here

1. [Quickstart](/aisw/quickstart/)  -  install, first profile, first switch
2. [Commands](/aisw/commands/)  -  complete syntax and flag reference
3. [How it works](/aisw/how-it-works/)  -  design decisions, credential storage, platform behavior
4. [Security](/aisw/security/)  -  local-only storage, keyring integration, file permissions
5. [Automation and scripting](/aisw/automation/)  -  CI patterns, JSON output, non-interactive mode
6. [Troubleshooting](/aisw/troubleshooting/)  -  common failures and diagnostics

## Additional reference

- [Adding profiles](/aisw/adding-profiles/)
- [Shell integration](/aisw/shell-integration/)
- [Supported tools](/aisw/supported-tools/)
- [Configuration](/aisw/configuration/)
- [Changelog](https://github.com/burakdede/aisw/releases)
