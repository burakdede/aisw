---
title: Commands
description: Full command reference for aisw commands, flags, and usage patterns.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/commands.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, Commands, reference, aisw command reference, aisw add use list status
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Commands","headline":"Commands","description":"Full command reference for aisw commands, flags, and usage patterns.","url":"https://burakdede.github.io/aisw/commands/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, aisw command reference, aisw add use list status","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.1.1","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Commands","item":"https://burakdede.github.io/aisw/commands/"}]}]}
---

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

For prompt behavior, JSON interfaces, stdout/stderr expectations, and automation-safe usage patterns, see [Automation and Scripting](/aisw/automation/).

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
```

---

## aisw use

Switch the active account for a tool.

```
aisw use <tool> <profile_name> [--state-mode <isolated|shared>]
```

`aisw use` applies the selected profile into the live config location each tool reads:
- Claude: live credentials file
- Codex: live `auth.json` plus file-store config in `~/.codex/config.toml`
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

Reports for each tool: whether the binary is installed, which profile is active, whether credential files are present, and whether the live tool config matches the configured active profile. Token validity, quota, and subscription state are not checked — aisw only verifies local file presence and that the local live state matches the selected profile.

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

Detects installed tools, installs the shell hook into your rc file, creates `~/.aisw/`, and offers to import any existing credentials. During interactive onboarding, imported profiles default to name `default` and label `imported`, but you can override both. Imported live credentials are marked active by default when no aisw-managed active profile already exists for that tool, and `aisw init` applies that active profile to the live tool config immediately. `aisw init --yes` stays deterministic and uses the default name and label. Safe to run multiple times — will not duplicate the shell hook.

When imported credentials are OAuth-based and aisw can resolve the authenticated account identity, it blocks importing a duplicate alias for an already stored account. If identity cannot be resolved, the import continues with a warning.

On success, `aisw init` prints a short next-step hint for reviewing or switching profiles.

Examples:

```
aisw init
aisw init --yes
```

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
