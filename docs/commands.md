# Command Reference

## Common workflows

### Add and activate a profile

Use this when you want to store a new account and switch to it immediately.

```
aisw add claude work --api-key sk-abc123 --label "Work account" --set-active
```

### List profiles as JSON

Use this when you want to inspect profiles from a script or another tool.

```
aisw list codex --json
```

### Restore a backup and re-apply it

Restoring a backup puts files back into the stored profile directory. Run `aisw use` after restore to apply that profile to the live tool config again.

```
aisw backup restore 20260325T114502Z-claude-work
aisw use claude work
```

## Automation and scripting

For prompt behavior, JSON interfaces, stdout/stderr expectations, and automation-safe usage patterns, see [Automation and Scripting](automation.md).

## aisw add

Add a new account profile for a tool.

```
aisw add <tool> <profile_name> [--api-key <key>] [--label <text>] [--set-active] [--from-live]
```

| Argument | Description |
|---|---|
| `tool` | `claude`, `codex`, or `gemini` |
| `profile_name` | Alphanumeric, hyphens, underscores. Max 32 characters. |

| Flag | Description |
|---|---|
| `--api-key <key>` | Provide the API key directly and skip the interactive prompt |
| `--label <text>` | Human-readable description stored with the profile |
| `--set-active` | Switch to this profile immediately after adding |
| `--from-env` | Read the API key from the tool's standard environment variable (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`) |
| `--from-live` | Capture whatever credentials the tool currently has in its live config and store them as a new aisw profile — no login flow is launched. The profile is always activated immediately, since the live credentials are already in use. Use this after a native `claude login` / `codex login` / `gemini login` when you want aisw to manage those credentials going forward. |
| `--yes` | Overwrite an existing same-name profile without prompting (only used with `--from-live`). Overwrite updates the existing profile in place; aisw does not delete the profile entry before capture succeeds. |

`--from-live` reads from each tool's live credential location:

| Tool | Source |
|---|---|
| `claude` | `~/.claude/.credentials.json`, or the system keyring on macOS |
| `codex` | `~/.codex/auth.json` |
| `gemini` | `~/.gemini/.env` (API key) or OAuth files in `~/.gemini/` |

When both Gemini `.env` and OAuth cache files are present, `aisw` uses `.env` first by design. This precedence is the same in `aisw add --from-live` and `aisw init`.

Without `--api-key`, `--from-env`, or `--from-live`, aisw presents an interactive menu to choose between browser OAuth login and API key entry.

For OAuth capture, `aisw` uses the narrowest upstream login flow it can:
- Claude: `claude auth login` with `CLAUDE_CONFIG_DIR` set to the capture directory
- Codex: `codex login --device-auth`
- Gemini: remains interactive for Google-account login because upstream headless mode requires preconfigured cached auth or env-based auth

For Claude OAuth, browser session state matters: Claude may reopen the account already signed in on `claude.com`. If you need a different Claude account, sign out of `claude.com` first and then rerun `aisw add claude <name>`.

On success, `aisw add` prints a short next-step hint for activating or verifying the new profile.

For OAuth profiles, aisw prevents duplicate aliases for the same resolved account identity when the stored credentials expose a reliable identifier. If identity cannot be resolved, the add still succeeds with a warning.

Examples:

```
aisw add claude work
aisw add codex personal --api-key sk-abc123
aisw add gemini team --label "Shared team key" --set-active
aisw add claude client-a --label "Client A OAuth account"
aisw add codex work --api-key sk-abc123 --label "OpenAI work key" --set-active
aisw add gemini backup --api-key AIza... --label "Backup quota account"
aisw add claude work --from-live
aisw add claude personal --from-live --label "Personal account" --set-active
aisw add codex work --from-live
aisw add gemini work --from-live --set-active
```

---

## aisw use

Switch the active account for a tool.

```
aisw use <tool> <profile_name> [--state-mode <isolated|shared>]
```

`aisw use` applies the selected profile into the live config location each tool reads:
- Claude: live credentials file or system keyring, depending on the live Claude auth backend
- Codex: live `auth.json` or system keyring, plus the matching auth-store config in `~/.codex/config.toml`
- Gemini: live `~/.gemini/.env` or token cache

`--state-mode` is supported for Claude and Codex:
- `isolated`: switch both account credentials and local tool state for that tool
- `shared`: keep the tool's shared local state and switch account credentials only

Gemini is currently isolated-only. `aisw` does not expose `--state-mode` for Gemini because the native `~/.gemini` directory mixes credentials with broader local state such as history, trusted folders, project mappings, settings, and MCP config. A Gemini "shared" mode would therefore mean sharing the whole native Gemini state, not just auth.

Normal switching does not require shell integration.

Examples:

```
aisw use claude work
aisw use claude work --state-mode shared
aisw use codex personal
aisw use codex personal --state-mode isolated
aisw use gemini default
aisw use claude backup
aisw use codex team-shared
```

---

## aisw list

Show all stored profiles.

```
aisw list [tool] [--json]
```

| Argument | Description |
|---|---|
| `tool` | Optional. Filter to `claude`, `codex`, or `gemini` |

| Flag | Description |
|---|---|
| `--json` | Output as a JSON array for scripting |

Examples:

```
aisw list
aisw list claude
aisw list codex --json
aisw list gemini --json
```

---

## aisw remove

Remove a stored profile.

```
aisw remove <tool> <profile_name> [--yes] [--force]
```

A final backup of the profile is always created before deletion. If the profile is currently active, `--force` is required.

| Flag | Description |
|---|---|
| `--yes` | Skip the confirmation prompt |
| `--force` | Allow removing the currently active profile |

Examples:

```
aisw remove codex backup
aisw remove claude old-work --yes
aisw remove gemini default --force
aisw remove codex team-shared --yes --force
```

---

## aisw rename

Rename a stored profile without recreating it.

```
aisw rename <tool> <old_name> <new_name>
```

Profile names must still be unique within a tool. Renaming an active profile preserves its active state under the new name.

Examples:

```
aisw rename claude default work
aisw rename codex personal oss
aisw rename gemini team backup
```

---

## aisw status

Show the current state across all tools.

```
aisw status [--json]
```

Reports for each tool: whether the binary is installed, which profile is active, which credential backend that profile uses, whether the managed credentials are present, and whether the live tool config matches the configured active profile. Token validity, quota, and subscription state are not checked — aisw only verifies local state and backend-specific credential presence.

For Claude and Codex, `status` also reports the active state mode (`isolated` or `shared`). Gemini does not currently support configurable state mode and remains isolated-only.

| Flag | Description |
|---|---|
| `--json` | Output as JSON |

Examples:

```
aisw status
aisw status --json
```

---

## aisw init

First-run setup.

```
aisw init
```

Detects installed tools, installs the shell hook into your rc file, creates `~/.aisw/`, and offers to import the current live credentials each upstream tool is using. During interactive onboarding, imported profiles default to name `default` and label `imported`, but you can override both. Imported live credentials are marked active by default when no aisw-managed active profile already exists for that tool, and `aisw init` applies that active profile to the live tool config immediately. `aisw init --yes` stays deterministic and uses the default name and label. Safe to run multiple times — will not duplicate the shell hook.

For Gemini, when both `.env` and OAuth cache files are present under `~/.gemini/`, import precedence is `.env` first.

`aisw init` reports current live upstream state, not a full inventory of every stored `~/.aisw` profile. If a tool's live account was changed outside `aisw`, `init` will report that current live account and note whether it matches the profile `aisw` records as active.

For Claude Code, `aisw init` distinguishes local Claude state from importable auth:
- file-backed Claude auth is imported from the live Claude config directory
- on macOS, Claude auth can also be imported from the `Claude Code-credentials` Keychain item when present
- if Claude local state exists but no importable auth is available, `aisw init` reports that explicitly instead of only saying credentials were “not found”

When imported credentials are OAuth-based and aisw can resolve the authenticated account identity, it blocks importing a duplicate alias for an already stored account. If identity cannot be resolved, the import continues with a warning.

On success, `aisw init` prints a short next-step hint for reviewing or switching profiles.

Examples:

```
aisw init
aisw init --yes
```

---

## aisw uninstall

Safely remove `aisw` shell integration and optionally delete `~/.aisw`.

```
aisw uninstall [--dry-run] [--remove-data] [--yes]
```

By default, `aisw uninstall` removes only the `aisw`-managed shell hook block from supported rc files and keeps `~/.aisw` intact. It does not modify upstream tool directories such as `~/.claude`, `~/.codex`, or `~/.gemini`.

Use `--dry-run` first to preview what will change. Use `--remove-data` if you also want to delete `~/.aisw` after shell integration is removed.

After uninstalling shell integration, `aisw` does not remove its own binary automatically. If you installed via Cargo, run `cargo uninstall aisw`. Otherwise remove the installed `aisw` binary manually.

| Flag | Description |
|---|---|
| `--dry-run` | Preview the uninstall plan without changing any files |
| `--remove-data` | Delete `~/.aisw` after removing shell integration |
| `--yes` | Skip the confirmation prompt |

Examples:

```
aisw uninstall --dry-run
aisw uninstall --yes
aisw uninstall --remove-data --yes
```

`aisw uninstall` removes only the `aisw` marker block it installed in bash, zsh, and fish rc files. It does not remove completions or the `aisw` binary itself; restart your shell or source the rc file after uninstalling, and remove the binary manually if you no longer want it installed.

---

## aisw shell-hook

Print the shell integration code for manual installation.

```
aisw shell-hook <shell>
```

`shell` must be `bash`, `zsh`, or `fish`.

Used internally by `aisw init`. Available separately if you prefer to manage your rc files manually:

```
aisw shell-hook zsh >> ~/.zshrc
aisw shell-hook bash >> ~/.bashrc
aisw shell-hook fish >> ~/.config/fish/conf.d/aisw.fish
```

---

## aisw backup

Manage credential backups. Backups are created automatically before every profile switch.

### aisw backup list

```
aisw backup list [--json]
```

Lists all backups with their unique backup id, tool, and profile name. Sorted newest-first.

| Flag | Description |
|---|---|
| `--json` | Output as a JSON array for scripting |

Examples:

```
aisw backup list
aisw backup list --json
```

### aisw backup restore

```
aisw backup restore <backup_id>
```

Restores credential files from a backup into the corresponding profile directory. Does not switch the active profile — run `aisw use` after restoring to apply the credentials.

On success, `aisw backup restore` prints a short next-step hint showing the exact `aisw use` command for the restored profile.

Examples:

```
aisw backup restore 20260325T114502Z-claude-work
aisw backup restore 20260325T114502Z-codex-personal
```
