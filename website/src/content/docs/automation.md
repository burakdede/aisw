---
title: Automation and Scripting
description: Understand prompt behavior, JSON output, stdout/stderr expectations, and safe scripting patterns for aisw.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/automation.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, Automation and Scripting, reference, aisw automation, aisw json output, aisw scripting, aisw stdout stderr
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Automation and Scripting","headline":"Automation and Scripting","description":"Understand prompt behavior, JSON output, stdout/stderr expectations, and safe scripting patterns for aisw.","url":"https://burakdede.github.io/aisw/automation/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, aisw automation, aisw json output, aisw scripting, aisw stdout stderr","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Automation and Scripting","item":"https://burakdede.github.io/aisw/automation/"}]}]}
---

Use this page when you want to run `aisw` safely from scripts, CI, or shell automation.

## Prompt behavior

`aisw` does not currently provide a global `--non-interactive` or `--quiet` flag.

Current automation-safe usage:

- Use `--yes` for commands that otherwise prompt for confirmation:
  - `aisw init --yes`
  - `aisw remove ... --yes`
  - `aisw backup restore ... --yes`
- Use `aisw add ... --api-key ...` when you need non-interactive profile creation. Without `--api-key`, `aisw add` uses an interactive auth flow.
- Use `--json` for machine-readable inventory and status output:
  - `aisw list --json`
  - `aisw status --json`
  - `aisw backup list --json`
- `aisw use --emit-env` and `aisw shell-hook` intentionally print raw shell output for scripting and shell integration.

If you need fully non-interactive automation today, prefer API-key-based `add`, explicit `--yes` flags, and the JSON output modes above.

## Output contract

`aisw` uses this output model:

- Human-oriented command results and status output go to stdout.
- Errors are printed to stderr and return a non-zero exit code.
- Interactive prompts appear during prompt-driven flows such as `init`, `remove`, and `backup restore` when you do not pass `--yes`.
- `--json` modes are the supported machine-readable interface for scripting.
- `aisw use --emit-env` and `aisw shell-hook` intentionally emit raw shell text to stdout.
- Commands that mutate `~/.aisw/config.json` take an exclusive config lock. If another `aisw` command is already updating config state, the later command waits briefly and then exits with a clear lock-timeout error instead of risking a lost update or partial overwrite.

Supported JSON interfaces:

- `aisw list --json`
- `aisw status --json`
- `aisw backup list --json`

JSON output is intended to remain stable for automation within a released major version. Human-readable stdout should be treated as presentation output and may change between releases.

## Practical patterns

### Create a profile without prompts

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
```

### Remove a profile non-interactively

```sh
aisw remove claude work --yes
```

### Read profile inventory from a script

```sh
aisw list --json
```

### Export shell variables for the selected profile

```sh
eval "$(aisw use codex work --emit-env)"
```

## Recommended references

- [Commands](/aisw/commands/) for the full command and flag reference
- [Shell Integration](/aisw/shell-integration/) for hook and completion setup
- [Quickstart](/aisw/quickstart/) for the common interactive workflow
