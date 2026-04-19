# Command Reference

Exact syntax and practical examples.

## Global flags

```text
aisw [--no-color] [--non-interactive] [--quiet] <command> ...
```

| Flag | Purpose |
|---|---|
| `--no-color` | Disable colored output |
| `--non-interactive` | Fail instead of prompting |
| `--quiet` | Suppress human-oriented presentation output |

## At a glance

```text
aisw init [--yes]
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--set-active]
aisw use <tool> <profile> [--state-mode isolated|shared]
aisw use --all --profile <profile>
aisw list [tool] [--json]
aisw status [--json]
aisw remove <tool> <profile> [--yes] [--force]
aisw rename <tool> <old> <new>
aisw backup list [--json]
aisw backup restore <backup_id> [--yes]
aisw uninstall [--dry-run] [--remove-data] [--yes]
aisw shell-hook <bash|zsh|fish>
aisw doctor [--json]
```

`tool` is one of: `claude`, `codex`, `gemini`.

## `aisw init`

```text
aisw init [--yes]
```

Bootstrap command:
- creates `~/.aisw/`
- offers shell hook setup
- offers importing existing live credentials

Notes:
- For Gemini, when both `.env` and OAuth cache files are present under `~/.gemini/`, import precedence is `.env` first.
- `aisw init` reports current live upstream state, not a full inventory of every stored `~/.aisw` profile.
- If a tool's live account was changed outside `aisw`, `init` reports that current live account and whether it matches the profile `aisw` records as active.
- For Claude Code, `init` distinguishes local Claude state from importable auth and reports when local state exists without importable credentials.
- When imported credentials are OAuth-based and aisw can resolve the authenticated account identity, it blocks importing a duplicate alias for an already stored account.

Examples:

```sh
aisw init
aisw init --yes
```

## Automation and scripting

For prompt behavior, JSON interfaces, stdout/stderr expectations, and automation-safe usage patterns, see [Automation and Scripting](automation.md).

## `aisw add`

```text
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--set-active]
```

| Flag | Purpose |
|---|---|
| `--api-key KEY` | Add with explicit API key |
| `--from-env` | Read key from tool env var (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`) |
| `--from-live` | Import the tool's current live credentials into aisw without launching login |
| `--label TEXT` | Add human-readable label |
| `--set-active` | Activate immediately after add |
| `--yes` | Overwrite existing profile name when used with `--from-live` |
Notes:
- Without `--api-key`, `--from-env`, or `--from-live`, add uses interactive auth flow.
- In `--non-interactive` mode, interactive add fails by design.
- `--from-live` reads the current native tool credentials and stores them as an aisw-managed profile.
- `--from-live` always activates the captured profile because those credentials are already live.
- With `--from-live --yes`, overwrite updates the existing profile in place; aisw does not delete the profile entry before capture succeeds.
Live credential sources:
- Claude: `~/.claude/.credentials.json`, or the system keyring on macOS
- Codex: `~/.codex/auth.json`
- Gemini: `~/.gemini/.env` or OAuth files in `~/.gemini/`
- When both Gemini `.env` and OAuth cache files are present, `aisw` uses `.env` first by design.

Examples:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex ci --from-env
aisw add gemini personal --label "Personal" --set-active
aisw add claude work --from-live
aisw add codex work --from-live --yes
```

## `aisw use`

```text
aisw use <tool> <profile> [--state-mode isolated|shared]
aisw use --all --profile <profile>
```

| Flag | Purpose |
|---|---|
| `--state-mode` | Claude/Codex only; `isolated` or `shared` |
| `--all` | Switch all tools in one command |
| `--profile` | Profile name used with `--all` |

Notes:
- `--state-mode` is not supported for Gemini.
- `--emit-env` exists but is internal/hidden and intended for shell-hook integration.

Examples:

```sh
aisw use claude work
aisw use codex work --state-mode shared
aisw use --all --profile personal
```

## `aisw list`

```text
aisw list [tool] [--json]
```

Examples:

```sh
aisw list
aisw list codex
aisw list --json
```

## `aisw status`

```text
aisw status [--json]
```

Shows per tool:
- installed binary detection
- active profile
- credential/backend state
- whether live config matches active profile

Examples:

```sh
aisw status
aisw status --json
```

## `aisw remove`

```text
aisw remove <tool> <profile> [--yes] [--force]
```

| Flag | Purpose |
|---|---|
| `--yes` | Skip confirmation |
| `--force` | Allow removing currently active profile |

Notes:
- A backup is created before deletion.

Examples:

```sh
aisw remove codex old --yes
aisw remove claude work --force --yes
```

## `aisw rename`

```text
aisw rename <tool> <old> <new>
```

Examples:

```sh
aisw rename claude default work
```

## `aisw backup`

### `aisw backup list`

```text
aisw backup list [--json]
```

Examples:

```sh
aisw backup list
aisw backup list --json
```

### `aisw backup restore`

```text
aisw backup restore <backup_id> [--yes]
```

| Flag | Purpose |
|---|---|
| `--yes` | Skip confirmation |

Notes:
- Restore writes files back into stored profile dir.
- Restore does not switch active profile; run `aisw use` after restore.

Example:

```sh
aisw backup restore 20260325T114502Z-claude-work --yes
aisw use claude work
```

## `aisw uninstall`

```text
aisw uninstall [--dry-run] [--remove-data] [--yes]
```

| Flag | Purpose |
|---|---|
| `--dry-run` | Preview changes |
| `--remove-data` | Remove `~/.aisw` after hook cleanup |
| `--yes` | Skip confirmation |

Notes:
- Removes only `aisw`-managed shell hook blocks.
- Does not remove tool configs (`~/.claude`, `~/.codex`, `~/.gemini`).
- Does not remove binary itself.

Examples:

```sh
aisw uninstall --dry-run
aisw uninstall --yes
aisw uninstall --remove-data --yes
```

## `aisw shell-hook`

```text
aisw shell-hook <bash|zsh|fish>
```

Examples:

```sh
aisw shell-hook zsh >> ~/.zshrc
aisw shell-hook bash >> ~/.bashrc
aisw shell-hook fish >> ~/.config/fish/conf.d/aisw.fish
```

## `aisw doctor`

```text
aisw doctor [--json]
```

Checks install and environment health.

Examples:

```sh
aisw doctor
aisw doctor --json
```

## Script-focused references

- [Automation and Scripting](automation.md)
- [Quickstart](quickstart.md)
- [Troubleshooting](troubleshooting.md)
