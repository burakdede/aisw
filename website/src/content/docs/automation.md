---
title: Automation and Scripting
description: Using aisw in CI pipelines, shell scripts, and non-interactive environments  -  flags, JSON output, exit codes, and common patterns.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/automation.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, automation and scripting, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Automation and Scripting","headline":"Automation and Scripting","description":"Using aisw in CI pipelines, shell scripts, and non-interactive environments  -  flags, JSON output, exit codes, and common patterns.","url":"https://burakdede.github.io/aisw/automation/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, automation and scripting, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.8","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Automation and Scripting","item":"https://burakdede.github.io/aisw/automation/"}]}]}
---

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
aisw --non-interactive backup restore 2026-03-25T11-45-02.123Z-0000 --yes
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
aisw backup restore 2026-03-25T11-45-02.123Z-0000 --yes --json
aisw verify --json
aisw repair --json --dry-run
aisw workspace bind --default --context work --json
aisw workspace unbind --default --json
aisw workspace guard --mode strict --json
aisw project-bindings list --json
aisw status --json
aisw status --context --json
aisw list --json
aisw list claude --json
aisw context list --json
aisw backup list --json
aisw doctor --json
```

With `--json`, success and expected command failures are emitted as structured JSON on stdout. Human-oriented stdout/stderr output is suppressed. The process still exits non-zero on failure.

Mutation results are wrapped in a top-level `result` object. For `use`,
`result.warnings` contains non-fatal diagnostics such as a failed OAuth
profile synchronization; an empty array means no such diagnostic was raised.
Warnings never include credential contents. In `--emit-env` mode, stdout
remains executable shell code and diagnostics stay on stderr.

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

# Record the installed Claude binary for compatibility diagnostics
aisw status --json | jq '.[] | select(.tool == "claude") | {binary_path, binary_version}'

# Get the derived active context name
aisw status --context --json | jq -r '.context.active'

# Check whether the live credentials match the active Claude profile
aisw status --json | jq '.[] | select(.tool == "claude") | .active_profile_applied'

# Get a one-shot pass/warn/fail verification verdict
aisw verify --json | jq -r '.summary.status'

# Preview safe local repairs and count remaining issues
aisw repair --json --dry-run | jq -r '.result.summary.issues_remaining'

# List all stored Codex profile names
# `list --json` is grouped by tool binary name, so index the tool key first.
# Antigravity is keyed as "agy".
aisw list codex --json | jq -r '.codex.profiles[].name'

# List all saved contexts
aisw context list --json | jq -r '.contexts[].name'

# Activate a saved context and read the refreshed active profile map
aisw context use work --json | jq '.result.active'

# Update the default workspace context and inspect the refreshed binding snapshot
aisw workspace bind --default --context work --json | jq '.result.project_bindings.user_bindings'

# Remove the default workspace context and inspect the refreshed binding snapshot
aisw workspace unbind --default --json | jq '.result.project_bindings.user_bindings'

# Persist strict workspace guard mode and confirm the saved mode
aisw workspace guard --mode strict --json | jq -r '.result.guard_mode'

# List user workspace rules plus the current repo-local binding
aisw project-bindings list --json | jq '.result'

# Find tools whose live credentials no longer match the recorded active profile
aisw status --json | jq '.[] | select(.active_profile_applied == false) | {tool, active_profile}'

# Find tools with missing credentials or overly broad permissions
aisw status --json | jq '.[] | select(.credentials_present == false or .permissions_ok == false) | .tool'

# Get the most recent backup for a specific profile
# Backup ids sort lexicographically, newest last.
aisw backup list --json | jq -r '[.[] | select(.tool == "claude" and .profile == "work")] | sort_by(.backup_id) | last | .backup_id'
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

## Exit codes

Exit code `0` means success. Any non-zero exit code means failure; the error message is on stderr in human mode, or in the JSON envelope on stdout in machine mode.

| Code | Meaning |
|---|---|
| `0` | Success |
| `2` | Profile not found  -  lets a script tell a wrong name from a real failure |
| `1` | Every other failure |

The JSON failure envelope carries the same information in a stable shape:

```json
{
  "ok": false,
  "error": {
    "kind": "profile_not_found",
    "message": "profile 'ghost' not found for claude.\n  Run 'aisw list claude' to see available profiles.",
    "exit_code": 2,
    "remediation": { "kind": "run_command", "command": "aisw list claude", "safe": true }
  }
}
```

Branch on `error.kind`, not on the message text  -  `kind` is stable, the message is not. `remediation` is present only when a suggested next command exists; `safe: true` means it is read-only and can be run automatically.

`aisw doctor` and `aisw verify` exit non-zero when a check fails, so they work directly as CI gates.

### `aisw use --all`

`--all` switches every tool that has a profile with the given name:

- A tool with no such profile is **skipped**, and the command still succeeds.
- A tool that has the profile but fails to switch is **reported**, and the command exits non-zero with the standard failure envelope.

So a zero exit means every tool that could switch did switch. If you need to know which tools were affected, read `result.affected_tools` from `--json` on success.

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

- [Commands](/aisw/commands/)  -  full flag reference
- [Shell integration](/aisw/shell-integration/)  -  hook installation and env var behavior
- [Quickstart](/aisw/quickstart/)  -  interactive usage reference
