---
title: Why aisw?
description: Why you need an AI agent account manager to bypass quotas and manage credentials securely.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/why-aisw.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, Why aisw?, overview, why use aisw, AI agent quota management, Claude Code quota switcher, Codex CLI quota switcher, Gemini CLI quota switcher
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Why aisw?","headline":"Why aisw?","description":"Why you need an AI agent account manager to bypass quotas and manage credentials securely.","url":"https://burakdede.github.io/aisw/why-aisw/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, why use aisw, AI agent quota management, Claude Code quota switcher, Codex CLI quota switcher, Gemini CLI quota switcher","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.0","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Why aisw?","item":"https://burakdede.github.io/aisw/why-aisw/"}]}]}
---

If you use AI coding agents like Claude Code, Codex CLI, or Gemini CLI, you eventually hit a usage limit. Whether it's a daily quota, a monthly token cap, or a rate limit, the result is the same: **your workflow stops.**

`aisw` was built to ensure your momentum isn't broken by subscription boundaries.

---

## The Problem: Friction and Risk

Before `aisw`, switching accounts was a manual, error-prone process:

- **Manual Credential Plumbing:** You had to find and move hidden files like `~/.claude/.credentials.json`, deal with macOS Keychain-backed Claude auth, or edit `~/.gemini/.env` manually.
- **Identity Confusion:** It was easy to lose track of which account was currently "live," leading to accidental usage of the wrong subscription.
- **Security Risks:** Manually copying sensitive credentials increases the risk of setting loose permissions or accidentally leaking them in a terminal history or git repo.
- **No Backups:** If you messed up a manual copy, your login session was gone, forcing you to re-run the browser OAuth flow.

---

## The Solution: Seamless, Secure Switching

`aisw` transforms account management into a single-command operation.

### 1. Atomic Switching
When you run `aisw use`, the change is atomic. We handle the file moves, permission settings, and environment variables in one shot. You don't have to remember where the tools store their secrets; `aisw` already knows.

### 2. Built-in Security
We treat your credentials as a "vault." All profiles are stored with `0600` (owner read/write only) permissions. `aisw status` acts as a security auditor, warning you if any live credential files have overly permissive access.

### 3. Identity Awareness
`aisw` isn't just a file copier; it's identity-aware. When you add or import a profile, it inspects the credentials to resolve a unique identity (like an email or account ID). This prevents you from creating duplicate aliases for the same account, keeping your profile list clean.

### 4. Safety Net (Automatic Backups)
Every time you switch profiles, `aisw` takes a snapshot of the current state. If a switch goes wrong or a tool's config format changes, you can restore your credentials with `aisw backup restore`.

---

## Comparison: Manual vs. aisw

| Feature | Manual Switching | Using `aisw` |
|---|---|---|
| **Speed** | 1-2 minutes (finding files, renaming) | < 2 seconds |
| **Reliability** | High risk of file corruption/loss | Atomic, verified operations |
| **Security** | Manual permission management | Automatic `0600` enforcement |
| **Organization** | Messy `.old` or `.backup` files | Named profiles with labels |
| **History** | None | Automatic switch snapshots |

---

## Who is it for?

- **Power Users:** Developers who maintain multiple subscriptions to bypass daily quotas.
- **Freelancers:** Engineers who need to switch between client-provided accounts and personal ones.
- **Teams:** Developers who share a "team" account but keep personal experiments on a separate profile.
- **Security-Conscious Users:** Anyone who wants a centralized, audited location for their AI agent credentials.

Ready to get started? [Head over to the Quickstart](/aisw/quickstart/).
