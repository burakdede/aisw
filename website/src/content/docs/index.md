---
title: aisw documentation
description: aisw manages named profiles and contexts for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI. Switch work, personal, and client accounts, then keep the right coding agent profile active per repo.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/index.md
template: splash
hero:
  title: "aisw"
  tagline: "Account manager and switcher for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI. Current release: v0.3.8."
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
      content: aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, aisw documentation, overview
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"WebPage","name":"aisw documentation","headline":"aisw documentation","description":"aisw manages named profiles and contexts for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI. Switch work, personal, and client accounts, then keep the right coding agent profile active per repo.","url":"https://burakdede.github.io/aisw/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, aisw documentation, overview","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.8","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"}]},{"@type":"FAQPage","mainEntity":[{"@type":"Question","name":"What does aisw actually change when I switch accounts?","acceptedAnswer":{"@type":"Answer","text":"aisw use applies the selected profile into the live config location that Claude Code, Codex CLI, or Gemini CLI already reads. It does not patch the tool binary, install a proxy, or change anything outside the relevant local credential and config files."}},{"@type":"Question","name":"Does aisw send credentials or prompts over the network?","acceptedAnswer":{"@type":"Answer","text":"No. aisw itself does not proxy requests, inspect prompts, or send your credentials to a remote service. It is a local credential and profile switcher."}},{"@type":"Question","name":"Where are profiles stored, and how are they protected?","acceptedAnswer":{"@type":"Answer","text":"Stored profiles live under ~/.aisw/profiles/<tool>/<name>/. Credential files are written with 0600 permissions so only your user can read or write them, and aisw status reports files that are broader than that."}}]}]}
---

Switch coding-agent accounts without copying credential files or logging in again every time.

`aisw` is a local profile and context manager for Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI. Save the accounts you already use, activate one tool or an entire work mode with a single command, and add repo-aware guardrails when the wrong account would be costly.

It is useful when you:

- keep separate work, personal, or client accounts;
- use several coding agents whose profile names do not line up; or
- want each repository to declare which account context it expects.

The core model is deliberately small:

- **Profiles** save one tool's account state.
- **Contexts** map one work mode to profiles across tools.
- **Workspace guardrails** warn or block launches when the active context is wrong.

aisw applies native credential state locally, takes a rollback snapshot before a switch, and reports whether live state still matches the profile you selected. It does not proxy model traffic, run a daemon, or send credentials to a service.

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

If you want the guided path, continue with [Quickstart](/aisw/quickstart/). If you are evaluating whether aisw fits a more complex setup, start with [Common switching situations](/aisw/common-situations/).

## Core workflow

```sh
# Store profiles
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal              # launches interactive OAuth
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"
aisw add antigravity work             # shared OAuth
aisw add antigravity api --api-key "$GEMINI_API_KEY"

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

For the mechanics behind profiles, contexts, and rollback, see [How aisw works](/aisw/how-it-works/). For provider-specific auth behavior and platform limits, see [Supported tools](/aisw/supported-tools/).

## See the workflow

These recordings walk through actual commands in isolated demo environments. They are useful for seeing the shape of the workflow before you install; the linked guides explain every decision and edge case.

**Switch, inspect, and recover a profile**

![Profile lifecycle demo](https://burakdede.github.io/aisw/demos/aisw-important-workflows.gif)

[Follow the Quickstart](/aisw/quickstart/) for the shortest path, or read [Adding profiles](/aisw/adding-profiles/) when you need to choose between OAuth, API keys, environment variables, and live imports.

**Group differently named accounts into one client context**

![Context workflow demo](https://burakdede.github.io/aisw/demos/aisw-context-workflow.gif)

[Learn about contexts](/aisw/common-situations/) when one work mode spans multiple providers.

**Prevent a wrong-account launch in a repository**

![Workspace guardrails demo](https://burakdede.github.io/aisw/demos/aisw-workspace-workflow.gif)

[Set up workspace guardrails](/aisw/workspace/) when the account associated with a repository matters as much as the code itself.

## Common situations

### Work and personal accounts for the same tool

Store both once, then switch by name:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal
aisw use claude work
```

See [Adding profiles](/aisw/adding-profiles/) for API keys, OAuth, live imports, and duplicate-account behavior.

### Mixed client setup across Claude, Codex, Gemini, and Antigravity

Use a context when each tool needs a different profile name:

```sh
aisw context create client-acme \
  --claude acme-claude \
  --codex client-a-openai \
  --gemini gemini-consulting \
  --antigravity acme-agy

aisw context use client-acme
```

A context only needs the tools you actually use  -  map one, or all four.

See [Common switching situations](/aisw/common-situations/) for the profile-versus-context decision and [Configuration](/aisw/configuration/) for the files and schemas involved.

### Wrong-account protection per repo

Use workspace guardrails when the repo itself should enforce the right work mode:

```sh
aisw workspace bind . --context client-acme
aisw workspace guard --mode strict
```

See [Workspace guardrails](/aisw/workspace/) for resolution order, remote patterns, path rules, and shell-hook behavior.

## Start here

| If you want to... | Start here | Then go deeper |
| --- | --- | --- |
| Install and switch your first account | [Quickstart](/aisw/quickstart/) | [Adding profiles](/aisw/adding-profiles/) |
| Choose between profiles, contexts, and guardrails | [Common switching situations](/aisw/common-situations/) | [Workspace guardrails](/aisw/workspace/) |
| Integrate aisw with a GUI, script, or CI job | [Automation and scripting](/aisw/automation/) | [Commands](/aisw/commands/) |
| Understand storage, rollback, and platform behavior | [How aisw works](/aisw/how-it-works/) | [Security](/aisw/security/) |
| Check support before installing | [Supported tools](/aisw/supported-tools/) | [Acceptance matrix](/aisw/acceptance-matrix/) |
| Diagnose a mismatch or failed switch | [Troubleshooting](/aisw/troubleshooting/) | [Configuration](/aisw/configuration/) |

## Additional reference

- [Adding profiles](/aisw/adding-profiles/)
- [Shell integration](/aisw/shell-integration/)
- [Workspace guardrails](/aisw/workspace/)
- [Why aisw](/aisw/why-aisw/)
- [Supported tools](/aisw/supported-tools/)
- [Configuration](/aisw/configuration/)
- [Changelog](https://github.com/burakdede/aisw/releases)
