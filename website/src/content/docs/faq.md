---
title: Frequently Asked Questions
description: Direct answers to common questions about switching Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI accounts with aisw.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/faq.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, frequently asked questions, overview
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Frequently Asked Questions","headline":"Frequently Asked Questions","description":"Direct answers to common questions about switching Claude Code, Codex CLI, Gemini CLI, and Antigravity CLI accounts with aisw.","url":"https://burakdede.github.io/aisw/faq/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, frequently asked questions, overview","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.8","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Frequently Asked Questions","item":"https://burakdede.github.io/aisw/faq/"}]}]}
---

Short answers to the questions people ask before installing `aisw`. Each answer links to the detailed workflow or reference that explains the behavior in full.

## How do I switch between two Claude Code accounts?

Add each account as a named profile, then activate the one you need:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal
aisw use claude work
aisw use claude personal
```

Start a new Claude process after switching. For OAuth, API keys, live imports, and Claude's shared-Keychain limitation, see [Adding profiles](/aisw/adding-profiles/) and [Supported tools](/aisw/supported-tools/).

## Can I manage multiple Codex CLI accounts on one machine?

Yes. API-key profiles and ChatGPT-managed profiles authenticated directly inside their own isolated `CODEX_HOME` are supported:

```sh
aisw add codex client-a
aisw add codex client-b
aisw use codex client-a
```

`aisw add codex <name> --from-live` is a bootstrap import for ChatGPT-managed auth, not the durable multi-profile setup. Shared-mode switching is intentionally blocked for that auth because Codex refreshes the session in place. See [Codex CLI details](supported-tools.md#codex-cli).

## How do I use different Claude, Codex, and Gemini accounts for one client?

Create a context that maps the client name to the correct profile for each tool:

```sh
aisw context create acme \
  --claude acme-claude \
  --codex acme-codex \
  --gemini acme-gemini

aisw context use acme
```

Contexts are references to profiles, not another credential store. They are the right choice when per-tool profile names differ. See [Common switching situations](/aisw/common-situations/) and [How aisw works](/aisw/how-it-works/).

## How do I prevent using my personal account in a client repository?

Bind the repository to its expected context and enable strict workspace guardrails:

```sh
aisw workspace bind . --context acme
aisw workspace guard --mode strict
```

With the shell hook installed, `claude`, `codex`, `gemini`, and `agy` are checked before launch. A mismatch blocks the agent and reports `aisw context use acme` as the remediation. See [Workspace guardrails](/aisw/workspace/).

## Does aisw send credentials or prompts to a server?

No. `aisw` is a local CLI: it does not proxy model traffic, upload credentials, inspect prompts, or run a remote service. It writes the upstream tool's local credential locations and uses the native OS keyring where supported. See [Security](/aisw/security/).

## What exactly changes when I run `aisw use`?

`aisw use` applies a managed profile's credential and tool state to the live locations that the upstream CLI reads, then records the active profile in `~/.aisw/config.json`. It does not modify the tool binary or alter prompts and conversations.

Before writing, aisw snapshots affected live state. If a write fails, it restores the snapshot and exits non-zero. A restored backup is not automatically active; run `aisw use` after `aisw backup restore`. See [How aisw works](how-it-works.md#atomic-switching-with-rollback).

## Can I capture an account that is already logged in?

Yes. Use `--from-live` to capture the currently active upstream state without launching another login flow:

```sh
aisw add claude work --from-live
aisw add codex work --from-live
aisw add gemini work --from-live
aisw add antigravity work --from-live
```

This is the quickest onboarding path. For durable ChatGPT-managed Codex profiles, authenticate directly inside the profile instead of relying on a shared live import. See [Adding profiles](adding-profiles.md#capture-current-live-credentials).

## Can a GUI or another program use aisw?

Yes. Use JSON output for structured results, stdin for secrets, and progress JSON for interactive OAuth:

```sh
printf '%s' "$OPENAI_API_KEY" | aisw add codex ci --api-key-stdin --json
aisw use codex ci --json
aisw verify --json
aisw add claude personal --progress-json
```

Integrations should read `aisw version --json` or `aisw capabilities --json`, branch on stable fields such as `error.kind`, and ignore additive fields. See [Automation and scripting](/aisw/automation/).

## Does aisw work on macOS, Linux, and Windows?

Yes, for the documented tool and auth paths. File-backed profiles work across platforms; native keyring support uses macOS Keychain, Linux Secret Service, and Windows Credential Manager. Provider-specific state-mode limits still apply, especially to Claude legacy shared-Keychain auth, ChatGPT-managed Codex shared mode, Gemini shared mode, and Antigravity isolation. See [Supported tools](/aisw/supported-tools/) and the [Acceptance Matrix](/aisw/acceptance-matrix/).

## What should I run when a switch looks wrong?

Run diagnostics in this order:

```sh
aisw doctor
aisw status --json
aisw verify --json
aisw repair --json --dry-run
```

`doctor` checks installation health, `status` shows live state and profile matches, `verify` gives a pass/warn/fail result, and `repair --dry-run` previews safe local repairs. See [Troubleshooting](/aisw/troubleshooting/).

## What does aisw not manage?

`aisw` manages local account state and switching; it does not manage provider billing, model traffic, upstream tool installation, prompts, extensions, themes, or expired-token refresh. Re-authentication remains the responsibility of the provider's native CLI flow. See [Why aisw](why-aisw.md#what-aisw-does-not-do).

## Further reading

- [Quickstart](/aisw/quickstart/) - install and complete the first switch
- [Commands](/aisw/commands/) - full syntax and flags
- [Adding profiles](/aisw/adding-profiles/) - credential sources and profile lifecycle
- [How aisw works](/aisw/how-it-works/) - storage, transactions, identity, and platform behavior
- [Automation and scripting](/aisw/automation/) - JSON contracts, exit codes, and CI patterns
