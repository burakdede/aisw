---
title: Why aisw
description: Why aisw exists  -  the problems with manual credential switching across Claude Code, Codex CLI, and Gemini CLI, and how named profiles solve them.
---

# Why aisw

## The problem

Claude Code, Codex CLI, and Gemini CLI each store credentials in different formats and different locations. When you need more than one account  -  work and personal, multiple clients, or team and individual licenses  -  you are left with manual options that all have the same failure mode: unclear state.

Editing `~/.claude/.credentials.json` directly is fragile. Copying and swapping files is fragile. Managing multiple `ANTHROPIC_API_KEY` exports in shell profiles for different terminals is fragile. None of these approaches tell you what is currently active, and none of them recover cleanly when something goes wrong mid-switch.

The problem compounds across tools. If you maintain accounts for all three CLIs, you are managing three separate hidden credential locations, three sets of manual procedures, and no unified view of what is active.

## What aisw does differently

`aisw` treats account switching as a named, stored, repeatable operation.

**Named profiles**  -  each account is given a name when added. `aisw add claude work --api-key "$ANTHROPIC_API_KEY"` captures a credential snapshot under the name `work`. Switching to it later is `aisw use claude work`.

**Atomic application**  -  switching takes a snapshot of the current live state before writing. If anything fails, the snapshot is restored. You do not end up mid-switch.

**Single status view**  -  `aisw status` shows every tool, its active profile, whether live credentials match the recorded active profile, and any expiry warnings  -  in one command.

**Cross-tool operations**  -  `aisw use --all --profile work` switches all three tools to the same profile name in a single command. `aisw list` and `aisw status --json` give a unified view across tools that works in scripts.

**Automatic backups**  -  `aisw remove` and `aisw rename` create backups before changing state. Backups are timestamped and restorable.

## What aisw does not do

- Does not proxy model traffic or inspect prompts.
- Does not manage tool settings, themes, keybindings, or extensions.
- Does not require a remote service or an account with `aisw`.
- Does not refresh expired OAuth tokens. That is the provider's responsibility.
- Does not invent credential locations. If a tool changes how it stores auth, `aisw` needs to be updated to match.

## Typical users

**Developers with separate work and personal accounts.** Work uses a team API key; personal uses an OAuth account. One command to switch all tools when stepping away from work context.

**Consultants and contractors.** Different client engagements use different provider accounts or API keys. Named profiles per client. Switching takes one command and leaves a clean audit trail via backups.

**Teams sharing accounts for specific tasks.** A shared team API key for CI or group work, individual OAuth accounts for everything else. `aisw` keeps both accessible without credential conflicts.

**Open-source contributors using multiple providers.** Development work uses a Claude personal account; reviewing changes or testing integrations uses a different provider or key. Named profiles make context switches deliberate.

## Design principles

**Fail closed.** If `aisw` cannot prove it knows where a tool stores its credentials, it does not guess. This matters for Codex's keyring-backed auth: keyring account identifiers can be opaque strings, not usernames. `aisw` will not fabricate an account name that might not match what Codex actually reads.

**Preserve native behavior.** Profile application writes exactly what the upstream tool would write if you authenticated natively. `aisw` does not introduce its own credential format or intermediary layer. The tool sees the same files and keychain entries it always expects.

**No ambient mutation.** `aisw` only touches credential locations when you explicitly run `aisw use` or `aisw add`. It does not run background processes or watch for credential changes outside of an active OAuth capture flow.
