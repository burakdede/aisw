---
title: Automation and scripting
description: Using aisw in CI pipelines, shell scripts, and non-interactive environments  -  flags, JSON output, exit codes, and common patterns.
---

# Automation and scripting

`aisw` is designed to be used safely in CI pipelines, shell scripts, and non-interactive environments.

## Baseline flags

```sh
aisw --non-interactive --quiet <command>
```

| Flag | Effect |
|---|---|
| `--non-interactive` | Fail instead of prompting. Safe for CI  -  commands that require user input will exit non-zero with a clear error. |
| `--quiet` | Suppress human-readable presentation output (tables, status lines). Does not suppress errors, JSON output, `--emit-env`, or `shell-hook` output. |
| `--yes` | Skip confirmation prompts on commands that ask before proceeding (remove, restore, uninstall). |

## Non-interactive patterns

```sh
# Add an API key profile without any prompts
aisw --non-interactive add claude ci --api-key "$ANTHROPIC_API_KEY"

# Or avoid passing the secret in argv
printf '%s' "$ANTHROPIC_API_KEY" | aisw --non-interactive add claude ci --api-key-stdin --json

# Add from an already-exported environment variable
aisw --non-interactive add codex ci --from-env

# Remove a profile with no confirmation
aisw --non-interactive remove codex ci --yes

# Restore a backup with no confirmation
aisw --non-interactive backup restore 20260325T114502Z-claude-ci --yes
```

Interactive OAuth flows (`aisw add claude personal` without flags) are not available in `--non-interactive` mode. Use `--api-key` or `--from-env` for CI.

## Machine-readable output

Read commands support `--json`, and core mutation commands now expose machine envelopes as well:

```sh
aisw version --json
aisw capabilities --json
aisw init --json --no-shell-hook --detect-live
aisw add claude work --api-key-stdin --json
aisw use claude work --json
aisw context create work --claude work-claude --codex work-codex --json
aisw context use work --json
aisw context rename work client-acme --json
aisw context remove client-acme --yes --json
aisw remove claude work --yes --json
aisw rename claude work personal --json
aisw backup restore 20260325T114502Z-claude-ci --yes --json
aisw verify --json
aisw repair --json --dry-run
aisw status --json
aisw status --context --json
aisw list --json
aisw list claude --json
aisw context list --json
aisw backup list --json
aisw doctor --json
```

With `--json`, success and expected command failures are emitted as structured JSON on stdout. Human-oriented stdout/stderr output is suppressed. The process still exits non-zero on failure.

For OAuth-based `add`, use `--progress-json` to stream newline-delimited JSON progress events:

```sh
aisw add claude personal --progress-json
```

Example event stream:

```json
{"type":"started","seq":1,"command":"add","tool":"claude","profile":"personal"}
{"type":"waiting_for_user","seq":3,"command":"add","tool":"claude","profile":"personal","phase":"waiting_for_user","safe_to_cancel":true,"message":"Complete login in the browser or terminal"}
{"type":"result","seq":5,"command":"add","tool":"claude","profile":"personal","ok":true,"result":{"tool":"claude","profile":"personal","auth_method":"oauth","credential_backend":"file","active":false,"source":null,"warnings":[]}}
```

### Useful JSON patterns

```sh
# Get the active Claude profile name from the plain status array
aisw status --json | jq -r '.[] | select(.tool == "claude") | .active_profile'

# Get the derived active context name
aisw status --context --json | jq -r '.context.active'

# Check whether the live credentials match the active Claude profile
aisw status --json | jq '.[] | select(.tool == "claude") | .active_profile_applied'

# Get a one-shot pass/warn/fail verification verdict
aisw verify --json | jq -r '.summary.status'

# Preview safe local repairs and count remaining issues
aisw repair --json --dry-run | jq -r '.result.summary.issues_remaining'

# List all stored Codex profile names
aisw list codex --json | jq -r '.profiles[].name'

# List all saved contexts
aisw context list --json | jq -r '.contexts[].name'

# Activate a saved context and read the refreshed active profile map
aisw context use work --json | jq '.result.active'

# Find profiles with expired tokens
aisw status --json | jq '.[] | select(.token_warning != null) | {tool, warning: .token_warning}'

# Get the most recent backup for a specific profile
aisw backup list --json | jq '[.[] | select(.profile == "claude/work")] | sort_by(.created_at) | last'
```

## Output contract

| Output | Destination | Notes |
|---|---|---|
| Human-readable tables and status | stdout | Suppressed by `--quiet` |
| Errors in human mode | stderr + non-zero exit | Always present, never suppressed |
| Errors in machine mode (`--json`, `--progress-json`) | stdout + non-zero exit | Structured JSON envelope |
| Prompts | stderr or tty | Only shown without `--non-interactive` and without `--yes` |
| `aisw use --emit-env` / `aisw context use --emit-env` | stdout | Shell variable exports; not affected by `--quiet` |
| `aisw shell-hook` | stdout | Shell hook code; not affected by `--quiet` |
| JSON output (`--json`) | stdout | Not affected by `--quiet` |
| Progress JSON (`--progress-json`) | stdout | One JSON object per line, intended for GUI/OAuth flows |

Exit code `0` means success. Any non-zero exit code means failure; the error message is on stderr.

## Applying profiles without the shell hook

If the shell hook is not installed, `aisw use` still writes live credential files and updates the active profile in config. For commands that need the env vars emitted by `aisw use`:

```sh
# Apply profile and capture env exports into the current shell
eval "$(aisw use codex work --emit-env)"

# Apply a saved cross-tool context into the current shell
eval "$(aisw context use acme --emit-env)"

# Or in a subshell
(eval "$(aisw use claude work --emit-env)"; claude ...)
```

`--emit-env` prints `export VAR=value` or `unset VAR` lines for any environment variables the activation sets (e.g. `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_API_KEY`).

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
active=$(aisw status --json | jq -r '.[] | select(.tool == "claude") | .active_profile')
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

- [Commands](commands.md)  -  full flag reference
- [Shell integration](shell-integration.md)  -  hook installation and env var behavior
- [Quickstart](quickstart.md)  -  interactive usage reference
