---
title: Automation and scripting
description: Using aisw in CI pipelines, shell scripts, and non-interactive environments — flags, JSON output, exit codes, and common patterns.
---

# Automation and scripting

`aisw` is designed to be used safely in CI pipelines, shell scripts, and non-interactive environments.

## Baseline flags

```sh
aisw --non-interactive --quiet <command>
```

| Flag | Effect |
|---|---|
| `--non-interactive` | Fail instead of prompting. Safe for CI — commands that require user input will exit non-zero with a clear error. |
| `--quiet` | Suppress human-readable presentation output (tables, status lines). Does not suppress errors, JSON output, `--emit-env`, or `shell-hook` output. |
| `--yes` | Skip confirmation prompts on commands that ask before proceeding (remove, restore, uninstall). |

## Non-interactive patterns

```sh
# Add an API key profile without any prompts
aisw --non-interactive add claude ci --api-key "$ANTHROPIC_API_KEY"

# Add from an already-exported environment variable
aisw --non-interactive add codex ci --from-env

# Remove a profile with no confirmation
aisw --non-interactive remove codex ci --yes

# Restore a backup with no confirmation
aisw --non-interactive backup restore 20260325T114502Z-claude-ci --yes
```

Interactive OAuth flows (`aisw add claude personal` without flags) are not available in `--non-interactive` mode. Use `--api-key` or `--from-env` for CI.

## Machine-readable output

All inventory and status commands support `--json`:

```sh
aisw status --json
aisw list --json
aisw list claude --json
aisw backup list --json
aisw doctor --json
```

JSON output goes to stdout. Errors always go to stderr with a non-zero exit code.

### Useful JSON patterns

```sh
# Get the active Claude profile name
aisw status --json | jq -r '.tools.claude.active_profile'

# Check whether the live credentials match the active profile
aisw status --json | jq '.tools.claude.live_match'

# List all stored Codex profile names
aisw list codex --json | jq -r '.[].name'

# Find profiles with expired tokens
aisw status --json | jq '.tools[] | select(.token_warning != null) | {tool, warning: .token_warning}'

# Get the most recent backup for a specific profile
aisw backup list --json | jq '[.[] | select(.profile == "claude/work")] | sort_by(.created_at) | last'
```

## Output contract

| Output | Destination | Notes |
|---|---|---|
| Human-readable tables and status | stdout | Suppressed by `--quiet` |
| Errors | stderr + non-zero exit | Always present, never suppressed |
| Prompts | stderr or tty | Only shown without `--non-interactive` and without `--yes` |
| `aisw use --emit-env` | stdout | Shell variable exports; not affected by `--quiet` |
| `aisw shell-hook` | stdout | Shell hook code; not affected by `--quiet` |
| JSON output (`--json`) | stdout | Not affected by `--quiet` |

Exit code `0` means success. Any non-zero exit code means failure; the error message is on stderr.

## Applying profiles without the shell hook

If the shell hook is not installed, `aisw use` still writes live credential files and updates the active profile in config. For commands that need the env vars emitted by `aisw use`:

```sh
# Apply profile and capture env exports into the current shell
eval "$(aisw use codex work --emit-env)"

# Or in a subshell
(eval "$(aisw use claude work --emit-env)"; claude ...)
```

`--emit-env` prints `export VAR=value` lines for any environment variables the profile activation sets (e.g. `CLAUDE_CONFIG_DIR`, `CODEX_HOME`).

## Concurrency

Commands that write `~/.aisw/config.json` take an exclusive file lock. If two `aisw` commands run concurrently, the second will wait briefly then fail with a lock error. This prevents partial writes in parallel CI matrix jobs. Design your CI steps so profile setup runs before parallel job steps that invoke the tools.

## Common CI patterns

### Set up a named profile in CI

```sh
# GitHub Actions or similar
- name: Configure Codex profile
  env:
    OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
  run: |
    aisw --non-interactive add codex ci --from-env
    aisw use codex ci
```

### Switch profile before a tool invocation

```sh
aisw --non-interactive use claude work
claude --print "summarize this file" < input.txt
```

### Verify active profile in a health check

```sh
active=$(aisw status --json | jq -r '.tools.claude.active_profile')
if [ "$active" != "ci" ]; then
  echo "Expected profile 'ci', got '${active}'" >&2
  exit 1
fi
```

### Clean up after CI

```sh
aisw --non-interactive remove codex ci --yes
```

## Related

- [Commands](commands.md) — full flag reference
- [Shell integration](shell-integration.md) — hook installation and env var behavior
- [Quickstart](quickstart.md) — interactive usage reference
