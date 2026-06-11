---
title: Commands
description: Complete syntax and flag reference for all aisw commands  -  add, use, list, status, remove, rename, backup, init, uninstall, shell-hook, and doctor.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/commands.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, commands, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Commands","headline":"Commands","description":"Complete syntax and flag reference for all aisw commands  -  add, use, list, status, remove, rename, backup, init, uninstall, shell-hook, and doctor.","url":"https://burakdede.github.io/aisw/commands/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, commands, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.6","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Commands","item":"https://burakdede.github.io/aisw/commands/"}]}]}
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
aisw init [--yes]
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--credential-backend file|system-keyring] [--set-active] [--yes]
aisw context create <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>]
aisw context list [--json]
aisw context use <name> [--state-mode isolated|shared]
aisw context set <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>]
aisw context unset <name> [--claude] [--codex] [--gemini]
aisw context remove <name> [--yes]
aisw context rename <old> <new>
aisw use <tool> <profile> [--state-mode isolated|shared]
aisw use --all --profile <profile> [--state-mode isolated|shared]
aisw list [tool] [--json]
aisw status [--context] [--json]
aisw remove <tool> <profile> [--yes] [--force]
aisw rename <tool> <old> <new>
aisw backup list [--json]
aisw backup restore <backup_id> [--yes]
aisw uninstall [--dry-run] [--remove-data] [--yes]
aisw shell-hook <bash|zsh|fish>
aisw doctor [--json]
```

`<tool>` is one of: `claude`, `codex`, `gemini`.

---

## `aisw init`

```text
aisw init [--yes]
```

Bootstrap command. Run once after install.

- Creates `~/.aisw/` with `0700` permissions.
- Offers shell hook installation for bash, zsh, or fish.
- Detects currently logged-in accounts for each tool and offers to import them as named profiles.
- Reports current live state per tool, including whether it matches any existing `aisw` profile.

| Flag | Effect |
|---|---|
| `--yes` | Accept all prompts without confirmation |

Notes:
- `init` is safe to re-run. If `~/.aisw/` already exists, it skips creation and proceeds to detection.
- For Gemini, when both `~/.gemini/.env` and OAuth cache files are present, import uses the `.env` file first.
- For Claude Code on macOS, `init` checks the Keychain before checking the credentials file.
- `init` will not import a duplicate if the OAuth identity matches an already-stored profile.

```sh
aisw init
aisw init --yes
```

---

## `aisw add`

```text
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--credential-backend file|system-keyring] [--set-active] [--yes]
```

Create a named profile.

| Flag | Effect |
|---|---|
| `--api-key KEY` | Store the given API key |
| `--from-env` | Read the key from the tool's env var (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`) |
| `--from-live` | Capture the tool's current live credentials without launching login |
| `--label TEXT` | Human-readable description, shown in `list` and `status` |
| `--credential-backend file|system-keyring` | Override where `aisw` stores the managed profile secret |
| `--set-active` | Activate the profile immediately after adding |
| `--yes` | Overwrite an existing profile when used with `--from-live` |

Notes:
- Without `--api-key`, `--from-env`, or `--from-live`, `add` runs the interactive OAuth flow for the tool.
- In `--non-interactive` mode, interactive OAuth is not available and the command fails.
- `--from-live` captures what the tool is currently using; it does not launch a browser or auth flow.
- `--from-live` always activates the profile because those credentials are already live.
- `--from-live --yes` overwrites an existing profile in place; the existing entry is not removed until capture succeeds.
- When OAuth identity can be resolved, `add` blocks creating a duplicate profile for an already-stored account.
- `--credential-backend` affects the managed `aisw` profile only. It does not force the upstream CLI's live auth backend.
- Gemini supports only `file`. Claude and Codex support `file` and `system-keyring`. Stored config and status output use `system_keyring`.

Live credential locations by tool:
- Claude: `~/.claude/.credentials.json` or the macOS Keychain
- Codex: `~/.codex/auth.json` or the OS keyring
- Gemini: `~/.gemini/.env` (API key) or OAuth files in `~/.gemini/`

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex ci --from-env
aisw add gemini personal --label "Personal account" --set-active
aisw add claude work --from-live
aisw add codex work --from-live --yes
```

---

## `aisw use`

```text
aisw use <tool> <profile> [--state-mode isolated|shared]
aisw use --all --profile <profile> [--state-mode isolated|shared]
```

Activate a stored profile as the live account.

| Flag | Effect |
|---|---|
| `--state-mode isolated` | Set `CLAUDE_CONFIG_DIR` or `CODEX_HOME` to the profile directory (default) |
| `--state-mode shared` | Unset `CLAUDE_CONFIG_DIR` or `CODEX_HOME`; tool reads its standard config dir |
| `--all` | Switch every tool that has a matching profile name |
| `--profile NAME` | Profile name; required with `--all` |

Notes:
- `--state-mode` applies to Claude Code and Codex CLI only. Gemini does not support it.
- Switching is atomic: the previous live state is snapshotted before any write. A failed write triggers a full rollback.
- With shell hook active, `aisw use` also emits the environment variable exports into the current shell session.

```sh
aisw use claude work
aisw use codex work --state-mode shared
aisw use --all --profile personal
```

---

## `aisw context`

Contexts are saved cross-tool mappings. They let you bind different per-tool profile names under one higher-level name such as `work`, `personal`, `client-acme`, or `oss`.

Practical framing:
- Use a `profile` when you want to switch one tool's account.
- Use a `context` when you want to switch one whole multi-tool work mode.

### `aisw context create`

```text
aisw context create <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>]
```

Create a saved context. At least one tool mapping is required.

```sh
aisw context create acme --claude acme-claude --codex acme-codex
```

### `aisw context list`

```text
aisw context list [--json]
```

List saved contexts.

```sh
aisw context list
aisw context list --json
```

### `aisw context use`

```text
aisw context use <name> [--state-mode isolated|shared]
```

Activate every mapped tool in a saved context as one transaction.

Notes:
- Default state mode is `isolated`.
- `--state-mode shared` applies only to Claude Code and Codex CLI.
- Activation is transactional across mapped tools. If one tool write fails, prior live state is restored.
- With the shell hook active, `aisw context use` applies emitted env vars to the current shell the same way `aisw use` does.

```sh
aisw context use acme
aisw context use acme --state-mode shared
```

### `aisw context set`

```text
aisw context set <name> [--claude <profile>] [--codex <profile>] [--gemini <profile>]
```

Update one or more mappings without disturbing the others.

```sh
aisw context set acme --gemini acme-gemini
```

### `aisw context unset`

```text
aisw context unset <name> [--claude] [--codex] [--gemini]
```

Remove one or more mappings from a context. The command fails if it would leave the context empty.

```sh
aisw context unset acme --codex
```

### `aisw context remove`

```text
aisw context remove <name> [--yes]
```

Delete a saved context. This does not change live credentials or active per-tool profiles.

```sh
aisw context remove acme --yes
```

### `aisw context rename`

```text
aisw context rename <old> <new>
```

Rename a saved context. This does not change live credentials or active per-tool profiles.

```sh
aisw context rename acme client-acme
```

---

## `aisw list`

```text
aisw list [tool] [--json]
```

Show all stored profiles.

```sh
aisw list
aisw list claude
aisw list --json
```

---

## `aisw status`

```text
aisw status [--context] [--json]
```

Show per-tool state: installed binary, active profile, credential backend, live-match status, and token expiry warnings.

Notes:
- "Live match" indicates whether the tool's current live credentials match the `aisw`-recorded active profile.
- Token expiry warnings appear when an OAuth token is expired or expires within 24 hours.
- `--context` adds derived context matching information without changing plain `status --json`.
- `status --context --json` wraps the existing tool array in a `{ "tools": [...], "context": ... }` object.

```sh
aisw status
aisw status --context
aisw status --json
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
aisw backup list [--json]
```

List available backups with timestamps and associated profile names.

```sh
aisw backup list
aisw backup list --json
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
aisw shell-hook <bash|zsh|fish>
```

Print the shell hook code for the given shell. Redirect into your shell config file:

```sh
aisw shell-hook zsh >> ~/.zshrc
aisw shell-hook bash >> ~/.bashrc
aisw shell-hook fish >> ~/.config/fish/conf.d/aisw.fish
```

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

## Automation reference

For CI patterns, JSON output contracts, and non-interactive usage, see [Automation and scripting](/aisw/automation/).
