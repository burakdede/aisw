---
title: Automation and Scripting
description: Non-interactive usage, JSON output, and scripting-safe patterns.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/automation.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, automation and scripting, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Automation and Scripting","headline":"Automation and Scripting","description":"Non-interactive usage, JSON output, and scripting-safe patterns.","url":"https://burakdede.github.io/aisw/automation/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, automation and scripting, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Automation and Scripting","item":"https://burakdede.github.io/aisw/automation/"}]}]}
---

Use this page for CI or script-safe `aisw` usage.

## Baseline flags

```text
aisw [--non-interactive] [--quiet] <command>
```

- `--non-interactive`: do not prompt; fail instead
- `--quiet`: suppress presentation output (does not suppress errors, JSON, `--emit-env`, or `shell-hook`)
- `--yes`: skip confirmation on commands that prompt

## Non-interactive patterns

```sh
# Add without OAuth/browser flow
aisw --non-interactive add codex ci --api-key "$OPENAI_API_KEY"

# Remove with no prompt
aisw --non-interactive remove codex ci --yes

# Restore backup with no prompt
aisw --non-interactive backup restore <backup_id> --yes
```

## Machine-readable output

Use `--json` for scripts:

```sh
aisw list --json
aisw status --json
aisw backup list --json
```

## Output contract

- human output: stdout
- errors: stderr + non-zero exit code
- prompts: shown only when allowed (no `--non-interactive` and no `--yes`)
- `aisw use --emit-env`: prints shell exports on stdout
- `aisw shell-hook`: prints shell hook code on stdout

## Concurrency

Commands that mutate `~/.aisw/config.json` take an exclusive lock. If another mutating command is already running, the later command times out with a lock error instead of writing partial state.

## Common script snippets

```sh
# Apply profile then run tool command
eval "$(aisw use codex work --emit-env)"

# Check whether expected profile is active
aisw status --json | jq -r '.tools.codex.active_profile'
```

## Related

- [Commands](/aisw/commands/)
- [Quickstart](/aisw/quickstart/)
- [Shell Integration](/aisw/shell-integration/)
