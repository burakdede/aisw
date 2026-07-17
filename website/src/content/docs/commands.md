---
title: Commands
description: Complete syntax and flag reference for all aisw commands  -  add, use, context, workspace, list, status, remove, rename, backup, init, uninstall, shell-hook, and doctor.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/commands.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, commands, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Commands","headline":"Commands","description":"Complete syntax and flag reference for all aisw commands  -  add, use, context, workspace, list, status, remove, rename, backup, init, uninstall, shell-hook, and doctor.","url":"https://burakdede.github.io/aisw/commands/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, commands, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.8","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Commands","item":"https://burakdede.github.io/aisw/commands/"}]}]}
---

## Global flags

```text
aisw [--no-color] [--non-interactive] [--quiet] <command> ...
```

| Flag | Effect |
|---|---|
| `--no-color` | Disable ANSI color output |
| `--non-interactive` | Fail instead of prompting; safe for CI |
| `--quiet` | Suppress human-readable presentation output; does not suppress errors, JSON output, `--emit-env`, or `shell-hook` |

## At a glance

```text
aisw init [--yes] [--json --no-shell-hook [--detect-live]]
aisw add <tool> <profile> [--api-key KEY|--api-key-stdin] [--from-env] [--from-live] [--label TEXT] [--credential-backend file|system-keyring] [--set-active] [--yes] [--json|--progress-json]
aisw context create <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>] [--json]
aisw context list [--search TEXT] [--json]
aisw context use <name> [--state-mode isolated|shared] [--emit-env] [--json]
aisw context set <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>] [--json]
aisw context unset <name> [--claude] [--codex] [--gemini] [--json]
aisw context remove <name> [--yes] [--json]
aisw context rename <old> <new> [--json]
aisw use <tool> <profile> [--state-mode isolated|shared] [--emit-env]
aisw use --all --profile <profile> [--state-mode isolated|shared] [--emit-env]
aisw workspace bind [PATH] --context <name> [--json]
aisw workspace bind --git-remote <PATTERN> --context <name> [--json]
aisw workspace bind --default --context <name> [--json]
aisw workspace unbind [PATH] [--json]
aisw workspace unbind --git-remote <PATTERN> [--json]
aisw workspace unbind --default [--json]
aisw workspace status [--json]
aisw workspace doctor [--json]
aisw workspace guard --mode warn|strict [--json]
aisw list [tool] [--tool <tool>] [--search TEXT] [--sort name|recent] [--active-only] [--json]
aisw status [--tool <tool>] [--search TEXT] [--sort name|recent] [--active-only] [--context] [--json]
aisw remove <tool> <profile> [--yes] [--force]
aisw rename <tool> <old> <new>
aisw backup list [--tool <tool>] [--search TEXT] [--sort name|recent] [--active-only] [--json]
aisw backup restore <backup_id> [--yes]
aisw uninstall [--dry-run] [--remove-data] [--yes]
aisw shell-hook <bash|zsh|fish|pwsh>
aisw doctor [--json]
aisw verify [--json]
aisw repair [--json] [--dry-run|--apply] [--fix home,permissions]
aisw project-bindings list [--json]
```

`<tool>` is one of: `claude`, `codex`, `gemini`.

---

## `aisw init`

```text
aisw init [--yes]
aisw init --json --no-shell-hook [--detect-live]
```

Bootstrap command. Run once after install.

- Creates `~/.aisw/` with `0700` permissions.
- Offers shell hook installation for bash, zsh, or fish.
- Detects currently logged-in accounts for each tool and offers to import them as named profiles.
- Reports current live state per tool, including whether it matches any existing `aisw` profile.

| Flag | Effect |
|---|---|
| `--yes` | Accept all prompts without confirmation |
| `--json` | Return a machine-readable bootstrap payload instead of interactive output |
| `--no-shell-hook` | Skip shell hook installation or modification; required with `--json` |
| `--detect-live` | Include live credential detection results in the machine payload |

Notes:
- `init` is safe to re-run. If `~/.aisw/` already exists, it skips creation and proceeds to detection.
- `init --json` is non-prompting by design. It creates `~/.aisw/config.json`, never edits shell rc files, and can report live credentials without importing them.
- For Gemini, when both `~/.gemini/.env` and OAuth cache files are present, import uses the `.env` file first.
- For Claude Code on macOS, `init` checks the Keychain before checking the credentials file.
- `init` will not import a duplicate if the OAuth identity matches an already-stored profile.

```sh
aisw init
aisw init --yes
aisw init --json --no-shell-hook --detect-live
```

---

## `aisw add`

```text
aisw add <tool> <profile> [--api-key KEY|--api-key-stdin] [--from-env] [--from-live] [--label TEXT] [--credential-backend file|system-keyring] [--set-active] [--yes] [--json|--progress-json]
```

Create a named profile.

| Flag | Effect |
|---|---|
| `--api-key KEY` | Store the given API key |
| `--api-key-stdin` | Read the API key from stdin until EOF |
| `--from-env` | Read the key from the tool's env var (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`) |
| `--from-live` | Capture the tool's current live credentials without launching login |
| `--label TEXT` | Human-readable description, shown in `list` and `status` |
| `--credential-backend file|system-keyring` | Override where `aisw` stores the managed profile secret |
| `--set-active` | Activate the profile immediately after adding |
| `--yes` | Overwrite an existing profile when used with `--from-live` |
| `--json` | Return a single machine-readable result envelope |
| `--progress-json` | Stream newline-delimited JSON progress events, then a final result event |

Notes:
- Without `--api-key`, `--from-env`, or `--from-live`, `add` runs the interactive OAuth flow for the tool.
- In `--non-interactive` mode, interactive OAuth is not available and the command fails.
- `--api-key-stdin` is intended for GUI and automation integrations that should not expose secrets in process arguments.
- `--from-live` captures what the tool is currently using; it does not launch a browser or auth flow.
- `--from-live` always activates the profile because those credentials are already live.
- `--from-live --yes` overwrites an existing profile in place; the existing entry is not removed until capture succeeds.
- For Codex ChatGPT-managed auth, `--from-live` is a bootstrap import, not a durable interchangeable account bundle.
- When OAuth identity can be resolved, `add` blocks creating a duplicate profile for an already-stored account.
- `--credential-backend` affects the managed `aisw` profile only. It does not force the upstream CLI's live auth backend.
- Gemini supports only `file`. Claude and Codex support `file` and `system-keyring`. Stored config and status output use `system_keyring`.

Live credential locations by tool:
- Claude: `~/.claude/.credentials.json` or the macOS Keychain
- Codex: `~/.codex/auth.json` or the OS keyring
- Gemini: `~/.gemini/.env` (API key) or OAuth files in `~/.gemini/`

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
printf '%s' "$ANTHROPIC_API_KEY" | aisw add claude work --api-key-stdin --json
aisw add codex ci --from-env
aisw add claude personal --progress-json
aisw add gemini personal --label "Personal account" --set-active
aisw add claude work --from-live
aisw add codex work --from-live --yes
```

---

## `aisw use`

```text
aisw use <tool> <profile> [--state-mode isolated|shared] [--emit-env]
aisw use --all --profile <profile> [--state-mode isolated|shared] [--emit-env]
```

Activate a stored profile as the live account.

| Flag | Effect |
|---|---|
| `--state-mode isolated` | Set `CLAUDE_CONFIG_DIR` or `CODEX_HOME` to the profile directory (default) |
| `--state-mode shared` | Unset `CLAUDE_CONFIG_DIR` or `CODEX_HOME`; tool reads its standard config dir |
| `--all` | Switch every tool that has a matching profile name |
| `--profile NAME` | Profile name; required with `--all` |
| `--emit-env` | Print shell export/unset lines to stdout instead of writing them to the session |

Notes:
- `--state-mode` applies to Claude Code and Codex CLI only. Gemini does not support it.
- Switching is atomic: the previous live state is snapshotted before any write. A failed write triggers a full rollback.
- With shell hook active, `aisw use` also emits the environment variable exports into the current shell session.
- `--emit-env` is used internally by the shell hook. You can use it directly to apply exports in a subshell: `eval "$(aisw use claude work --emit-env)"`.
- Codex shared mode remains supported for API-key profiles.
- Codex shared mode is blocked for ChatGPT-managed auth. Use `--state-mode isolated` instead; this is an expected upstream limitation, not `aisw` corruption.

```sh
aisw use claude work
aisw use codex work --state-mode shared
aisw use --all --profile personal
eval "$(aisw use claude work --emit-env)"
```

---

## `aisw context`

Contexts are saved cross-tool mappings. They let you bind different per-tool profile names under one higher-level name such as `work`, `personal`, `client-acme`, or `oss`.

Practical framing:
- Use a `profile` when you want to switch one tool's account.
- Use a `context` when you want to switch one whole multi-tool work mode.

### `aisw context create`

```text
aisw context create <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>] [--json]
```

Create a saved context. At least one tool mapping is required.

```sh
aisw context create acme --claude acme-claude --codex acme-codex
aisw context create acme --claude acme-claude --json
```

### `aisw context list`

```text
aisw context list [--search TEXT] [--json]
```

List saved contexts.

| Flag | Effect |
|---|---|
| `--search TEXT` | Filter by context name or mapped profile name |
| `--json` | Output as JSON |

```sh
aisw context list
aisw context list --search acme
aisw context list --json
```

### `aisw context use`

```text
aisw context use <name> [--state-mode isolated|shared] [--emit-env] [--json]
```

Activate every mapped tool in a saved context as one transaction.

| Flag | Effect |
|---|---|
| `--state-mode isolated` | Set `CLAUDE_CONFIG_DIR` and `CODEX_HOME` to profile directories (default) |
| `--state-mode shared` | Unset `CLAUDE_CONFIG_DIR` and `CODEX_HOME` for Claude and Codex |
| `--emit-env` | Print shell export/unset lines to stdout instead of writing them to the session |
| `--json` | Output a machine-readable activation result envelope |

Notes:
- Default state mode is `isolated`.
- `--state-mode shared` applies only to Claude Code and Codex CLI.
- For Codex, `--state-mode shared` is blocked when the mapped profile uses ChatGPT-managed auth.
- Activation is transactional across mapped tools. If one tool write fails, prior live state is restored.
- With the shell hook active, `aisw context use` applies emitted env vars to the current shell the same way `aisw use` does.

```sh
aisw context use acme
aisw context use acme --state-mode shared
aisw context use acme --json
eval "$(aisw context use acme --emit-env)"
```

### `aisw context set`

```text
aisw context set <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>] [--json]
```

Update one or more mappings without disturbing the others.

```sh
aisw context set acme --gemini acme-gemini
aisw context set acme --gemini acme-gemini --json
```

### `aisw context unset`

```text
aisw context unset <name> [--claude] [--codex] [--gemini] [--json]
```

Remove one or more mappings from a context. The command fails if it would leave the context empty.

```sh
aisw context unset acme --codex
aisw context unset acme --codex --json
```

### `aisw context remove`

```text
aisw context remove <name> [--yes] [--json]
```

Delete a saved context. This does not change live credentials or active per-tool profiles.

```sh
aisw context remove acme --yes
aisw context remove acme --yes --json
```

### `aisw context rename`

```text
aisw context rename <old> <new> [--json]
```

Rename a saved context. This does not change live credentials or active per-tool profiles.

```sh
aisw context rename acme client-acme
aisw context rename acme client-acme --json
```

---

## `aisw workspace`

Bind repos, directories, and git remotes to expected `aisw` contexts. The shell hook checks these bindings before launching `claude`, `codex`, or `gemini`, warning or blocking when the active context does not match.

See [Workspace guardrails](/aisw/workspace/) for a full explanation of the feature, setup steps, and common patterns.

### `aisw workspace bind`

```text
aisw workspace bind [PATH] --context <name> [--json]
aisw workspace bind --git-remote <PATTERN> --context <name> [--json]
aisw workspace bind --default --context <name> [--json]
```

Create or update a workspace binding. The context must already exist.

| Flag | Effect |
|---|---|
| `PATH` | Path to bind. Defaults to `.`. Inside a git repo, writes `.git/info/aisw.json`. Outside a repo, writes a path rule to `~/.aisw/workspaces.json`. |
| `--context NAME` | Expected context name for this location |
| `--git-remote PATTERN` | Bind by git remote URL pattern. Supports `*` wildcards. |
| `--default` | Set the fallback context for locations with no more specific rule. |
| `--json` | Output a machine-readable mutation envelope with the refreshed bindings snapshot |

```sh
aisw workspace bind . --context client-acme
aisw workspace bind --git-remote "github.com/acme/*" --context client-acme
aisw workspace bind ~/clients --context client-acme
aisw workspace bind --default --context personal
aisw workspace bind --default --context personal --json
```

### `aisw workspace unbind`

```text
aisw workspace unbind [PATH] [--json]
aisw workspace unbind --git-remote <PATTERN> [--json]
aisw workspace unbind --default [--json]
```

Remove an existing workspace binding.

| Flag | Effect |
|---|---|
| `PATH` | Path to unbind. Defaults to `.`. Inside a git repo, removes `.git/info/aisw.json`. Outside a repo, removes the matching path rule from `~/.aisw/workspaces.json`. |
| `--git-remote PATTERN` | Remove a git remote rule. Supports the same normalization as `bind`, so `git@github.com:acme/*` and `github.com/acme/*` target the same rule. |
| `--default` | Clear the fallback context for locations with no more specific rule. |
| `--json` | Output a machine-readable mutation envelope with the refreshed bindings snapshot |

Notes:
- `unbind` errors if the targeted binding does not exist.
- When invoked in a git repo without `PATH`, repo-local unbind removes `.git/info/aisw.json`.

```sh
aisw workspace unbind .
aisw workspace unbind ~/clients/acme-api
aisw workspace unbind --git-remote "github.com/acme/*"
aisw workspace unbind --default --json
```

### `aisw workspace status`

```text
aisw workspace status [--json]
```

Show the resolved binding for the current directory: matched rule, expected context, active context/profiles, status, and recommended action.

```sh
aisw workspace status
aisw workspace status --json
```

### `aisw workspace doctor`

```text
aisw workspace doctor [--json]
```

Validate all workspace rules. Checks that referenced context names still exist and reports the resolved state for the current directory.

```sh
aisw workspace doctor
aisw workspace doctor --json
```

### `aisw workspace guard`

```text
aisw workspace guard --mode warn|strict [--json]
```

Set the default guard mode, saved to `~/.aisw/workspaces.json`.

With `--json`, the success envelope includes the updated `guard_mode` and the same bindings snapshot returned by `aisw project-bindings list --json`.

| Mode | Effect |
|---|---|
| `warn` | Print a warning before launching an agent. The launch proceeds. (Default) |
| `strict` | Block the agent launch entirely and print a remediation command. |

```sh
aisw workspace guard --mode warn
aisw workspace guard --mode strict
aisw workspace guard --mode strict --json
```

---

## `aisw list`

```text
aisw list [tool] [--tool <tool>] [--search TEXT] [--sort name|recent] [--active-only] [--json]
```

Show all stored profiles. Pass a tool name as a positional argument or use `--tool` to filter to one tool.

| Flag | Effect |
|---|---|
| `[tool]` or `--tool` | Filter to one tool: `claude`, `codex`, or `gemini` |
| `--search TEXT` | Filter by profile name or label (substring match) |
| `--sort name\|recent` | Sort by profile name or by most recently used |
| `--active-only` | Show only tools that have an active profile |
| `--json` | Output as JSON |

```sh
aisw list
aisw list claude
aisw list --tool codex --search work
aisw list --sort recent
aisw list --active-only --json
```

---

## `aisw status`

```text
aisw status [--tool <tool>] [--search TEXT] [--sort name|recent] [--active-only] [--context] [--json]
```

Show per-tool state: installed binary, active profile, credential backend, live-match status, and token expiry warnings.

| Flag | Effect |
|---|---|
| `--tool` | Filter to one tool: `claude`, `codex`, or `gemini` |
| `--search TEXT` | Filter by tool, profile, auth type, or backend text |
| `--sort name\|recent` | Sort rows by name or most recently used |
| `--active-only` | Show only tools that have an active profile |
| `--context` | Add derived context matching information |
| `--json` | Output as JSON |

Notes:
- "Live match" indicates whether the tool's current live credentials match the `aisw`-recorded active profile.
- Token expiry warnings appear when an OAuth token is expired or expires within 24 hours.
- `--context` does not change the shape of plain `status --json` output.
- `status --context --json` wraps the tool array in a `{ "tools": [...], "context": ... }` object.

```sh
aisw status
aisw status --context
aisw status --tool claude
aisw status --active-only
aisw status --search work --json
aisw status --context --json
```

---

## `aisw remove`

```text
aisw remove <tool> <profile> [--yes] [--force]
```

Delete a stored profile. A backup is created before deletion.

| Flag | Effect |
|---|---|
| `--yes` | Skip confirmation prompt |
| `--force` | Allow removing the currently active profile |

```sh
aisw remove codex old --yes
aisw remove claude work --force --yes
```

---

## `aisw rename`

```text
aisw rename <tool> <old> <new>
```

Rename a profile. The profile directory and all config references are updated atomically.

```sh
aisw rename claude default work
```

---

## `aisw backup list`

```text
aisw backup list [--tool <tool>] [--search TEXT] [--sort name|recent] [--active-only] [--json]
```

List available backups with timestamps and associated profile names.

| Flag | Effect |
|---|---|
| `--tool` | Filter to one tool: `claude`, `codex`, or `gemini` |
| `--search TEXT` | Filter by backup id, tool, or profile name |
| `--sort name\|recent` | Sort by name or by most recently created |
| `--active-only` | Show only backups for currently active profiles |
| `--json` | Output as JSON |

```sh
aisw backup list
aisw backup list --tool claude
aisw backup list --search work --json
aisw backup list --sort recent
```

---

## `aisw backup restore`

```text
aisw backup restore <backup_id> [--yes]
```

Restore profile files from a backup. Does not activate the profile; run `aisw use` after restore.

| Flag | Effect |
|---|---|
| `--yes` | Skip confirmation prompt |

```sh
aisw backup restore 20260325T114502Z-claude-work --yes
aisw use claude work
```

---

## `aisw uninstall`

```text
aisw uninstall [--dry-run] [--remove-data] [--yes]
```

Remove `aisw`-managed shell hook blocks from shell config files.

| Flag | Effect |
|---|---|
| `--dry-run` | Preview what would be changed without making any changes |
| `--remove-data` | Also remove `~/.aisw/` after hook cleanup |
| `--yes` | Skip confirmation prompt |

Notes:
- Does not remove the `aisw` binary.
- Does not remove tool config directories (`~/.claude/`, `~/.codex/`, `~/.gemini/`).
- Only removes `# aisw` hook blocks that `aisw init` or `aisw shell-hook` added.

```sh
aisw uninstall --dry-run
aisw uninstall --yes
aisw uninstall --remove-data --yes
```

---

## `aisw shell-hook`

```text
aisw shell-hook <bash|zsh|fish|pwsh>
```

Print the shell hook code for the given shell. Redirect into your shell config file:

```sh
aisw shell-hook zsh >> ~/.zshrc
aisw shell-hook bash >> ~/.bashrc
aisw shell-hook fish >> ~/.config/fish/conf.d/aisw.fish
aisw shell-hook pwsh >> $PROFILE
```

The hook does two things:
1. Wraps `aisw use` and `aisw context use` so environment variable exports are applied into the current shell session automatically.
2. Wraps `claude`, `codex`, and `gemini` to run `aisw workspace check` before each launch, enforcing any configured workspace guardrails.

See [Shell integration](/aisw/shell-integration/) for details and completion setup.

---

## `aisw doctor`

```text
aisw doctor [--json]
```

Check install and environment health: binary locations, `~/.aisw/` permissions, shell hook status, and keyring availability.

```sh
aisw doctor
aisw doctor --json
```

---

## `aisw verify`

```text
aisw verify [--json]
```

Read-only confidence check that combines installation health with live profile coherence.

- Reuses `doctor` checks for binaries, config, shell hook, keyring, and permissions.
- Verifies whether each active tool's live credentials still match the profile `aisw` records as active.
- Returns non-zero when concrete failures are found, such as live mismatch, missing managed credentials, or missing binaries.

| Flag | Effect |
|---|---|
| `--json` | Output a machine-readable verification report |

Notes:
- `verify` is stricter than `status --json`: it adds an overall pass/warn/fail verdict and remediation hints.
- `verify` is read-only. It never reapplies credentials or modifies shell files.
- On macOS, Claude file-backed live verification can remain observational because the live Keychain state is not always inspectable.

```sh
aisw verify
aisw verify --json
```

---

## `aisw repair`

```text
aisw repair [--json] [--dry-run|--apply] [--fix home,permissions]
```

Preview or apply safe local repairs for aisw-managed state.

- `home`: create `AISW_HOME` and a default `config.json` when missing
- `permissions`: normalize aisw-managed directories to `0700` and files to `0600` on Unix

| Flag | Effect |
|---|---|
| `--json` | Output a machine-readable repair result envelope |
| `--dry-run` | Preview repair actions without mutating files |
| `--apply` | Apply the selected safe fixes |
| `--fix` | Limit repairs to one or more fix categories; accepts comma-separated values |

Notes:
- If neither `--dry-run` nor `--apply` is provided, `repair` defaults to dry-run mode.
- `repair` is explicit and cautious. It does not reapply live credentials, restore backups, or modify shell rc files.
- `--fix` defaults to all currently safe repair categories.

```sh
aisw repair
aisw repair --json --dry-run
aisw repair --apply --fix home
aisw repair --json --apply --fix home,permissions
```

---

## `aisw project-bindings list`

```text
aisw project-bindings list [--json]
```

List workspace binding rules that matter to GUI/project-aware flows.

- Includes saved `guard_mode`
- Includes user-level workspace rules from `~/.aisw/workspaces.json`
- Includes the current repo-local binding from `.git/info/aisw.json` when the current directory is inside a repo
- Does not scan the filesystem for arbitrary repo-local binding files outside the current repo

```sh
aisw project-bindings list
aisw project-bindings list --json
```

---

## Automation reference

For CI patterns, JSON output contracts, and non-interactive usage, see [Automation and scripting](/aisw/automation/).
