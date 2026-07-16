---
title: Why aisw?
description: Why aisw exists  -  the problems with manual credential switching across Claude Code, Codex CLI, and Gemini CLI, and why named profiles, contexts, and guardrails fit those workflows better.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/why-aisw.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, why aisw?, overview
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Why aisw?","headline":"Why aisw?","description":"Why aisw exists  -  the problems with manual credential switching across Claude Code, Codex CLI, and Gemini CLI, and why named profiles, contexts, and guardrails fit those workflows better.","url":"https://burakdede.github.io/aisw/why-aisw/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, why aisw?, overview","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.7","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Why aisw?","item":"https://burakdede.github.io/aisw/why-aisw/"}]}]}
---

## The problem

Claude Code, Codex CLI, and Gemini CLI each store credentials in different formats and different locations. When you need more than one account  -  work and personal, multiple clients, or team and individual licenses  -  you are left with manual options that all have the same failure mode: unclear state.

Editing `~/.claude/.credentials.json` directly is fragile. Copying and swapping files is fragile. Managing multiple `ANTHROPIC_API_KEY` exports in shell profiles for different terminals is fragile. None of these approaches tell you what is currently active, and none of them recover cleanly when something goes wrong mid-switch. For Codex ChatGPT-managed auth, copied shared session state is also not a durable primitive because upstream refreshes that session in place.

The problem compounds across tools. If you maintain accounts for all three CLIs, you are managing three separate hidden credential locations, three sets of manual procedures, and no unified view of what is active.

That is why people usually discover `aisw` through a very concrete search, not an abstract architecture question. They search for things like:

- "switch between two Claude Code accounts"
- "multiple Codex CLI accounts on one machine"
- "Gemini CLI work and personal profile switch"
- "coding agent account per repo"

Those are all versions of the same underlying need: make account state explicit, repeatable, and safe.

## What aisw does differently

`aisw` treats account switching as a named, stored, repeatable operation.

**Named profiles**  -  each account is given a name when added. `aisw add claude work --api-key "$ANTHROPIC_API_KEY"` captures a credential snapshot under the name `work`. Switching to it later is `aisw use claude work`.

**Atomic application**  -  switching takes a snapshot of the current live state before writing. If anything fails, the snapshot is restored. You do not end up mid-switch.

**Single status view**  -  `aisw status` shows every tool, its active profile, whether live credentials match the recorded active profile, and any expiry warnings  -  in one command.

**Cross-tool operations**  -  `aisw use --all --profile work` switches all three tools to the same profile name in a single command when those names line up. `aisw list` and `aisw status --json` give a unified view across tools that works in scripts.

**Saved contexts**  -  `aisw context create acme --claude acme-claude --codex acme-codex --gemini acme-gemini` captures a real-world mixed-account setup under one reusable name. `aisw context use acme` restores that whole work mode as one transaction, without forcing each tool to share the same profile name.

**Automatic backups**  -  `aisw remove` and `aisw rename` create backups before changing state. Backups are timestamped and restorable.

**Workspace guardrails**  -  `aisw workspace bind . --context client-acme` ties a repo to an expected context. The shell hook then checks the binding before each `claude`, `codex`, or `gemini` launch and warns or blocks when the active context does not match.

**Machine-readable integration surface**  -  `aisw version --json`, `aisw capabilities --json`, mutation JSON output, `--api-key-stdin`, and `--progress-json` make `aisw` usable as the switching layer behind a GUI or another local automation client without changing the human CLI workflow.

## Profiles vs contexts

This distinction matters because the two features solve different problems.

**Profiles solve the per-tool account problem.**

A profile is one saved account snapshot for one tool.

Use a profile when your problem sounds like:
- "I have a work Claude account and a personal Claude account."
- "I need a CI Codex API key that is separate from my local account."
- "I want to capture the Gemini account I already authenticated with."

Benefits of profiles:
- One stable name for one tool's credential state.
- Safe switching with rollback for that tool.
- Per-tool backups, status, and lifecycle commands.

Limits of profiles:
- They do not express a cross-tool work mode by themselves.
- `claude/work`, `codex/work`, and `gemini/work` only line up cleanly when you intentionally gave them the same name.
- Once your real setup becomes `acme-claude`, `acme-openai`, and `acme-gemini`, you need a context.

**Contexts solve the cross-tool work-mode problem.**

A context is one saved mapping from tool to profile name.

Use a context when your problem sounds like:
- "My acme setup uses `acme-claude`, `acme-codex`, and `acme-gemini`."
- "I want `work`, `personal`, or `client-acme` to mean one whole multi-tool setup, not one profile per tool."
- "I switch between real project modes, not just between individual accounts."

Benefits of contexts:
- One name for a real multi-tool state.
- Transactional activation across all mapped tools.
- Better status visibility for whether your current per-tool state still matches a saved work mode.

Limits of contexts:
- They do not contain credentials.
- They do not replace vendor auth flows.
- They are only references to existing per-tool profiles.

## What aisw does not do

- Does not proxy model traffic or inspect prompts.
- Does not manage tool settings, themes, keybindings, or extensions.
- Does not require a remote service or an account with `aisw`.
- Does not refresh expired OAuth tokens. That is the provider's responsibility.
- Does not invent credential locations. If a tool changes how it stores auth, `aisw` needs to be updated to match.

## Typical users

**Developers with separate work and personal accounts.** Work uses a team API key; personal uses an OAuth account. One command to switch all tools when stepping away from work context.

**Consultants and contractors.** Different client engagements use different provider accounts or API keys. Named profiles per client, contexts to group them, and workspace guardrails to prevent launching the wrong account in the wrong repo. Switching takes one command and leaves a clean audit trail via backups.

**People building local GUI tooling on top of existing CLIs.** `aisw` already knows how to capture, store, switch, verify, and repair upstream credential state. A GUI can treat it as the local credential authority instead of re-implementing tool-specific auth behavior.

**Teams sharing accounts for specific tasks.** A shared team API key for CI or group work, individual OAuth accounts for everything else. `aisw` keeps both accessible without credential conflicts.

**Open-source contributors using multiple providers.** Development work uses a Claude personal account; reviewing changes or testing integrations uses a different provider or key. Named profiles make context switches deliberate.

## Design principles

**Fail closed.** If `aisw` cannot prove it knows where a tool stores its credentials, it does not guess. This matters for Codex's keyring-backed auth: keyring account identifiers can be opaque strings, not usernames. `aisw` will not fabricate an account name that might not match what Codex actually reads.

**Preserve native behavior.** Profile application writes exactly what the upstream tool would write if you authenticated natively. `aisw` does not introduce its own credential format or intermediary layer. The tool sees the same files and keychain entries it always expects.

**No ambient mutation.** `aisw` only touches credential locations when you explicitly run `aisw use` or `aisw add`. It does not run background processes or watch for credential changes outside of an active OAuth capture flow.

**Add machine interfaces without breaking terminal habits.** Human-readable CLI output stays human-readable. Structured JSON, typed errors, and progress streams are additive, explicit machine modes rather than a redesign of the interactive CLI.

## Common questions

**How do I switch between two Claude Code accounts?**

Install `aisw`, add each account as a named profile, and switch with one command:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal   # prompts for OAuth
aisw use claude work
aisw use claude personal
```

**Can I manage multiple Codex CLI accounts?**

Yes, with an important distinction:

- API-key profiles are fully durable.
- ChatGPT-managed Codex profiles are durable when each profile authenticates directly inside its own isolated `CODEX_HOME`.
- `aisw add codex <name> --from-live` remains supported as a bootstrap import, but not as a durable interchangeable shared-session bundle.
- Shared-mode ChatGPT switching is intentionally blocked because that upstream auth is refreshed in place.

**Can I manage multiple Gemini CLI accounts?**

Yes. Gemini uses an OAuth token stored locally. `aisw add gemini <name>` captures it; `aisw use gemini <name>` restores it.

**What if my work setup uses Claude, Codex, and Gemini with different account names?**

Use a context. A context maps one name to a set of per-tool profiles:

```sh
aisw context create acme --claude acme-claude --codex acme-codex --gemini acme-gemini
aisw context use acme
```

**Is it safe to use? Does it send credentials anywhere?**

No. `aisw` is fully local. Credentials never leave your machine. There is no remote service, no analytics, and no credential proxy. See [Security](/aisw/security/) for details.

**What if I accidentally launch an agent with the wrong account?**

Workspace guardrails prevent this. Bind a repo to an expected context; the shell hook warns or blocks agent launches when the active context does not match. See [Workspace guardrails](/aisw/workspace/).
