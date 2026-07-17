---
title: Common switching situations
description: Practical aisw workflows for switching Claude Code, Codex CLI, and Gemini CLI accounts across work, personal, client, and repo-based setups.
---

# Common switching situations

Most people do not go looking for a "profile manager." They go looking for a fix to a specific daily problem:

- "How do I switch between two Claude Code accounts?"
- "How do I keep a client Codex account separate from my personal one?"
- "How do I stop launching the wrong Gemini or Claude account in the wrong repo?"

This page is the shortest path from those problems to the `aisw` feature that actually solves them.

## One tool, two accounts

This is the most common starting point: one tool, one work account, one personal account.

Example with Claude Code:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add claude personal

aisw use claude work
aisw use claude personal
```

What this solves:

- You stop editing `~/.claude/.credentials.json` manually.
- You stop wondering which account is currently active.
- You get a named switch instead of a one-off shell trick you need to remember later.

The same pattern works for Codex CLI and Gemini CLI.

## Same profile name across every tool

Sometimes the simple case is real: every tool should use the same named account mode, such as `work` or `personal`.

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"

aisw use --all --profile work
```

Use this when you want one command to switch all supported coding agents to the same conceptual mode and the names already line up naturally.

## Different profile names for the same client or workspace

Real setups often do not line up neatly. A client workspace might use:

- `acme-claude`
- `client-a-openai`
- `gemini-consulting`

That is where contexts matter:

```sh
aisw context create client-acme \
  --claude acme-claude \
  --codex client-a-openai \
  --gemini gemini-consulting

aisw context use client-acme
```

Use a context when the thing you are switching is not "one tool account" but "one whole work mode."

## Capture what is already live

Sometimes the account you want is already logged in. You do not need to re-authenticate just to start managing it.

```sh
aisw add claude work --from-live
aisw add codex consulting --from-live
aisw add gemini personal --from-live
```

This is especially useful when:

- You already signed in through the native upstream CLI.
- You are onboarding `aisw` onto an existing machine.
- You want to preserve the current known-good state before changing anything.

For Codex ChatGPT-managed auth, `--from-live` is bootstrap-only. After import, re-login directly inside the isolated profile if you want a durable profile that survives future upstream refreshes cleanly.

For Gemini, this pattern applies to whichever auth mode is currently live on the machine. Upstream Gemini docs currently recommend Google-account login for interactive local use, but some account types still require `GOOGLE_CLOUD_PROJECT`, and non-interactive/headless use should rely on `GEMINI_API_KEY` or Vertex AI.

## GUI-safe and automation-safe secret entry

If another application is driving `aisw`, passing API keys in process arguments is the wrong shape. `aisw` supports stdin-based secret entry for that path:

```sh
printf '%s' "$ANTHROPIC_API_KEY" | aisw add claude work --api-key-stdin --json
```

Use this when:

- You are building a desktop app on top of `aisw`.
- You are calling `aisw` from another process and do not want the secret in argv.
- You want structured success or failure output back.

Related machine-mode commands:

```sh
aisw version --json
aisw capabilities --json
aisw verify --json
```

## Keep the right account active per repo

If you work across personal repos, employer repos, and client repos, the expensive mistake is not "switching is inconvenient." It is "I launched the right tool with the wrong account."

Bind a repo to an expected context:

```sh
cd ~/clients/acme-api
aisw workspace bind . --context client-acme
aisw workspace guard --mode strict
```

With the shell hook installed, `aisw` checks the expected context before `claude`, `codex`, or `gemini` launches. That makes workspace guardrails the answer to searches like:

- "coding agent account switch per repo"
- "prevent wrong Claude account in client repository"
- "different AI CLI accounts for different projects"

## Verify that switching really worked

People rarely want switching by itself. They want confidence.

```sh
aisw status
aisw status --context
aisw verify --json
aisw repair --json --dry-run
```

Use `verify` when you want a machine-readable confidence check after a switch. Use `repair --dry-run` when you want to see what `aisw` believes is fixable before letting it mutate anything.

## Which feature should I reach for?

Use a profile when:

- One tool needs more than one account.
- You want to switch Claude Code, Codex CLI, or Gemini CLI individually.
- The main question is "which account should this one tool use?"

Use a context when:

- One workspace spans multiple tools.
- Per-tool profile names differ.
- The main question is "which whole setup should I be in right now?"

Use workspace guardrails when:

- The repo itself should enforce the expected account mode.
- A wrong-account launch is a real risk.
- You want warnings or hard blocks before an agent starts.

## Next steps

- [Quickstart](quickstart.md)
- [Why aisw](why-aisw.md)
- [Workspace guardrails](workspace.md)
- [Automation and scripting](automation.md)
