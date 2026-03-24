# Command Reference

## aisw add

Add a new account profile for a tool.

```
aisw add <tool> <profile_name> [--api-key <key>] [--label <text>] [--set-active]
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

Without `--api-key`, aisw presents an interactive menu to choose between browser OAuth login and API key entry.

Examples:

```
aisw add claude work
aisw add codex personal --api-key sk-abc123
aisw add gemini team --label "Shared team key" --set-active
```

---

## aisw use

Switch the active account for a tool.

```
aisw use <tool> <profile_name>
```

For Claude and Codex, the switch requires shell integration to take effect in the current session. If the shell hook is not installed, aisw prints the manual fallback command. Run `aisw init` to install the hook.

For Gemini, the switch rewrites `~/.gemini/.env` directly — no shell hook required.

Examples:

```
aisw use claude work
aisw use codex personal
aisw use gemini default
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
```

---

## aisw status

Show the current state across all tools.

```
aisw status [--json]
```

Reports for each tool: whether the binary is installed, which profile is active, whether credential files are present, and whether the expected environment variables are set. Token validity is not checked — aisw only verifies that files exist.

| Flag | Description |
|---|---|
| `--json` | Output as JSON |

---

## aisw init

First-run setup.

```
aisw init
```

Detects installed tools, installs the shell hook into your rc file, creates `~/.aisw/`, and offers to import any existing credentials as a profile named `default`. Safe to run multiple times — will not duplicate the shell hook.

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
```

---

## aisw backup

Manage credential backups. Backups are created automatically before every profile switch.

### aisw backup list

```
aisw backup list
```

Lists all backups with their timestamp, tool, and profile name. Sorted newest-first.

### aisw backup restore

```
aisw backup restore <timestamp>
```

Restores credential files from a backup into the corresponding profile directory. Does not switch the active profile — run `aisw use` after restoring to apply the credentials.
