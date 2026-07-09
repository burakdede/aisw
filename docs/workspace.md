---
title: Workspace guardrails
description: Bind repos, directories, and git remotes to expected aisw contexts. Get warnings or hard blocks when the wrong account is active before launching claude, codex, or gemini.
---

# Workspace guardrails

Workspace guardrails solve one specific problem: launching an AI coding agent in the wrong repo with the wrong account.

If you work on client repos alongside personal projects, you may have noticed that `claude`, `codex`, and `gemini` are always willing to start regardless of which account is currently active. Nothing stops you from opening a client repo and accidentally running it under your personal key - or vice versa.

The workspace feature lets you bind a repo or directory to an expected `aisw` context. The shell hook then checks that binding before each agent launch and either warns you or blocks the launch entirely.

## Concepts

**Workspace binding** maps a location to an expected context. The location can be:
- A specific git repo (stored locally in `.git/info/aisw.json`, never committed)
- A directory path or path prefix on your filesystem
- A git remote URL pattern (`github.com/acme/*`)
- The fallback default for any location not matched by a more specific rule

**Guard mode** controls what happens when the active context does not match the expected one:
- `warn` (default): print a warning before the agent launches. The launch proceeds.
- `strict`: block the launch entirely with a remediation command.

**Resolution order** (most specific wins):
1. Repo-local binding (`.git/info/aisw.json`)
2. User path rule (`~/.aisw/workspaces.json`, longest prefix match)
3. User git-remote rule (most-specific pattern match)
4. Default context

## Setup

### Step 1: Create contexts for your workspaces

Workspace guardrails work on top of existing contexts. Create a context for each work mode you want to protect:

```sh
# Work at a client using a dedicated Claude team account
aisw context create client-acme \
  --claude acme-claude \
  --codex acme-codex \
  --gemini acme-gemini

# Personal projects using personal accounts
aisw context create personal \
  --claude personal-claude \
  --codex personal-codex \
  --gemini personal-gemini
```

### Step 2: Install the shell hook

The workspace check runs as part of the shell hook. Install or update it:

```sh
# Bash
echo 'eval "$(aisw shell-hook bash)"' >> ~/.bashrc && source ~/.bashrc

# Zsh
echo 'eval "$(aisw shell-hook zsh)"' >> ~/.zshrc && source ~/.zshrc

# Fish
echo 'aisw shell-hook fish | source' >> ~/.config/fish/config.fish

# PowerShell
Add-Content $PROFILE "`naisw shell-hook pwsh | Out-String | Invoke-Expression"
```

### Step 3: Bind your repos

Bind the current repo to a context:

```sh
cd ~/clients/acme-api
aisw workspace bind . --context client-acme
```

This writes `.git/info/aisw.json` inside the repo. The file is in the git directory, not the working tree, so it is never committed or pushed.

Bind by git remote pattern to cover all repos under an organization:

```sh
aisw workspace bind --git-remote "github.com/acme/*" --context client-acme
```

Bind a filesystem path prefix:

```sh
aisw workspace bind ~/clients --context client-acme
```

Set a default context for any location without a more specific binding:

```sh
aisw workspace bind --default --context personal
```

### Step 4: Set guard mode

```sh
# Print a warning but allow the launch (default)
aisw workspace guard --mode warn

# Block the launch and require explicit context switch
aisw workspace guard --mode strict
```

## How it works in practice

Once the shell hook is active and a binding is set, the `claude`, `codex`, and `gemini` commands in your shell become wrapper functions. Each time you run one of them, the hook calls `aisw workspace check` first.

With `warn` mode, in a mismatched repo you see:

```
Workspace guard warning: expected context 'client-acme', current state 'personal-claude, personal-codex, personal-gemini' (mismatch). Run 'aisw context use client-acme' before launching claude.
```

The agent still launches. You can proceed if the mismatch is intentional.

With `strict` mode:

```
Error: workspace guard refused to launch claude.
  Expected context: 'client-acme'
  Current state: 'personal-claude, personal-codex, personal-gemini'
  Status: mismatch
  Run 'aisw context use client-acme'.
```

The agent does not launch. Fix the context and try again:

```sh
aisw context use client-acme
claude
```

The prompt check also runs when you change directories, so you get a heads-up as soon as you `cd` into a bound repo rather than only when you try to launch an agent.

## Commands

### `aisw workspace bind`

```text
aisw workspace bind [PATH] --context <name> [--json]
aisw workspace bind --git-remote <PATTERN> --context <name> [--json]
aisw workspace bind --default --context <name> [--json]
```

Create or update a workspace binding.

Pass `--json` when driving the command from a GUI or script that needs a structured mutation result and refreshed bindings snapshot.

| Flag | Effect |
|---|---|
| `PATH` | Path to bind. Defaults to `.`. Inside a git repo, writes to `.git/info/aisw.json`. Outside a repo, writes a path rule to `~/.aisw/workspaces.json`. |
| `--context NAME` | Context to expect at this location. Must already exist. |
| `--git-remote PATTERN` | Bind by git remote URL pattern. Supports `*` wildcards. Cannot be combined with `PATH` or `--default`. |
| `--default` | Set the fallback context for locations without a more specific rule. Cannot be combined with `PATH` or `--git-remote`. |

Repo-local bindings take priority over all user-level rules. They are written to `.git/info/aisw.json`, which is excluded from the working tree by default and never committed.

```sh
# Bind the current repo locally
aisw workspace bind . --context client-acme

# Bind a specific repo by path
aisw workspace bind ~/clients/acme-api --context client-acme

# Bind all repos under a GitHub organization by remote pattern
aisw workspace bind --git-remote "github.com/acme/*" --context client-acme

# Bind a path prefix (any subdirectory of ~/work maps to the work context)
aisw workspace bind ~/work --context work

# Set a default context for unlisted locations
aisw workspace bind --default --context personal
```

### `aisw workspace status`

```text
aisw workspace status [--json]
```

Show the resolved binding for the current directory: which rule matched, which context is expected, what is currently active, and what command to run if there is a mismatch.

```sh
aisw workspace status
aisw workspace status --json
```

Example output:

```
Workspace
  Workspace:      /Users/alice/clients/acme-api
  Repo root:      /Users/alice/clients/acme-api
  Expected:       client-acme
  Active:         claude acme-claude, codex acme-codex, gemini acme-gemini
  Active context: client-acme
  Status:         match
  Matched rule:   repo_local:/Users/alice/clients/acme-api/.git/info/aisw.json
```

JSON output:

```json
{
  "workspace": "/Users/alice/clients/acme-api",
  "repo_root": "/Users/alice/clients/acme-api",
  "matched_rule": "repo_local:/Users/alice/clients/acme-api/.git/info/aisw.json",
  "expected_context": "client-acme",
  "active_context": "client-acme",
  "active_profiles": {
    "claude": "acme-claude",
    "codex": "acme-codex",
    "gemini": "acme-gemini"
  },
  "status": "match",
  "recommended_command": null
}
```

Status values:

| Status | Meaning |
|---|---|
| `match` | Active profiles match the expected context. |
| `mismatch` | A binding is set but the active profiles do not match. |
| `no_expected_context` | No binding applies to the current location. |
| `invalid_context` | The bound context name no longer exists. |
| `ambiguous_active` | Multiple contexts match the current active profiles. |
| `unmanaged` | A binding is set but no tool has an active profile. |

### `aisw workspace doctor`

```text
aisw workspace doctor [--json]
```

Validate workspace rules and the current workspace state. Checks that every context referenced by a rule still exists, and reports the resolved status for the current directory.

```sh
aisw workspace doctor
aisw workspace doctor --json
```

### `aisw workspace guard`

```text
aisw workspace guard --mode warn|strict [--json]
```

Set the default guard mode. The setting is saved to `~/.aisw/workspaces.json`.

With `--json`, the success envelope includes the saved `guard_mode` and the refreshed project bindings snapshot.

```sh
aisw workspace guard --mode warn    # warn but allow launch
aisw workspace guard --mode strict  # block launch on mismatch
```

## Common scenarios

### You work in client repos and a personal repo

```sh
# Create one context per work mode
aisw context create acme --claude acme-claude --codex acme-codex --gemini acme-gemini
aisw context create personal --claude personal-claude --codex personal-codex --gemini personal-gemini

# Bind each client repo locally
cd ~/clients/acme-api && aisw workspace bind . --context acme
cd ~/clients/acme-ui && aisw workspace bind . --context acme

# Set personal as the default for everything else
aisw workspace bind --default --context personal

# Use strict mode so mistakes are caught before they happen
aisw workspace guard --mode strict
```

### You have many repos under a GitHub organization

```sh
# Instead of binding each repo individually, bind by remote pattern
aisw workspace bind --git-remote "github.com/acme-corp/*" --context acme-corp
```

Any repo with a remote matching that pattern will resolve to the `acme-corp` context automatically, even repos you clone in the future.

### You want to check state without launching an agent

```sh
aisw workspace status
aisw workspace status --json
```

### You temporarily need to override the guard

In `warn` mode, just proceed - the warning is informational. In `strict` mode, switch context first:

```sh
aisw context use personal
claude  # launches with personal context
```

To switch back:

```sh
aisw context use acme
```

## Storage

Workspace configuration is stored in two places:

| Location | Contains |
|---|---|
| `.git/info/aisw.json` | Repo-local binding. Stays with the repo's git directory. Never committed. |
| `~/.aisw/workspaces.json` | User-level path rules, git-remote rules, default context, and guard mode. |

Both files have `0600` permissions on Unix systems.

## Related

- [Commands](commands.md) - full flag reference including `workspace check`
- [Shell integration](shell-integration.md) - how the prompt check and agent wrappers work
- [Quickstart](quickstart.md) - getting started with profiles and contexts
