---
title: Shell integration
description: Install and configure the aisw shell hook for bash, zsh, fish, and PowerShell. Understand what the hook does, how workspace guardrails work, and how shell completions work.
---

# Shell integration

The shell hook is optional. Without it, `aisw use` and `aisw context use` still write live tool credential files and update `~/.aisw/config.json`. The hook adds two capabilities:

1. Applying environment variable exports (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_API_KEY`) into the current shell session when you run `aisw use` or `aisw context use`.
2. Wrapping `claude`, `codex`, and `gemini` so that [workspace guardrails](workspace.md) are enforced before each launch.

## Install

### Zsh

Add to `~/.zshrc`:

```zsh
eval "$(aisw shell-hook zsh)"
```

Then reload:

```sh
source ~/.zshrc
```

### Bash

Add to `~/.bashrc` (interactive shells) or `~/.bash_profile`:

```bash
eval "$(aisw shell-hook bash)"
```

Then reload:

```sh
source ~/.bashrc
```

### Fish

Add to `~/.config/fish/config.fish`:

```fish
aisw shell-hook fish | source
```

Or as a standalone file:

```sh
aisw shell-hook fish > ~/.config/fish/conf.d/aisw.fish
```

### PowerShell

Add to your PowerShell profile (`$PROFILE`):

```powershell
aisw shell-hook pwsh | Out-String | Invoke-Expression
```

To find or create your profile file:

```powershell
# Check if a profile exists
Test-Path $PROFILE

# Create one if it does not exist
New-Item -ItemType File -Force $PROFILE

# Append the hook
Add-Content $PROFILE "`naisw shell-hook pwsh | Out-String | Invoke-Expression"
```

Reload the profile:

```powershell
. $PROFILE
```

`AISW_SHELL` is set to `pwsh` automatically by the hook so `aisw` emits PowerShell-compatible `$env:VAR = '...'` syntax instead of POSIX `export`.

## Verify

```sh
echo "$AISW_SHELL_HOOK"
# Expected: 1
```

## What the hook does

The hook installs two sets of wrappers in your shell.

### Profile switching wrappers

The `aisw` shell function intercepts `aisw use ...` and `aisw context use ...`:

1. Runs the command with `--emit-env` to write live credential files and print shell exports to stdout.
2. Evals those exports so `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, and `GEMINI_API_KEY` are set immediately in the current session.
3. Passes all other subcommands through to the binary unchanged.

Without the hook, you can do this manually:

```sh
eval "$(aisw use claude work --emit-env)"
eval "$(aisw context use acme --emit-env)"
```

`context use` defaults to `isolated` state mode. Pass `--state-mode shared` when you want Claude Code or Codex CLI to use their standard config directories instead of the profile-specific ones.

### Workspace guard wrappers

The hook also wraps `claude`, `codex`, and `gemini`. Before each launch, it runs `aisw workspace check --tool <tool>`. If a workspace binding is set for the current directory and the active context does not match:

- In `warn` mode: a warning is printed to stderr. The agent launches anyway.
- In `strict` mode: the launch is blocked with an error and a remediation command.

The hook also runs a directory-change check (`chpwd` in zsh, `PROMPT_COMMAND` in bash, `--on-variable PWD` in fish). When you `cd` into a bound repo, you see the workspace mismatch warning at the prompt before you even type a command.

See [Workspace guardrails](workspace.md) for setup and full details.

## Remove

Remove the hook line from your shell config and open a new shell. For bash, zsh, and fish, `aisw uninstall` can do this automatically:

```sh
aisw uninstall --dry-run    # preview
aisw uninstall --yes        # apply
```

For PowerShell, remove the `Invoke-Expression` line from `$PROFILE` manually and reload.

## Shell completions

`aisw` ships completion scripts for bash, zsh, and fish. They are installed automatically by the Homebrew formula and shell installer.

### Installed locations

| Shell | Path |
|---|---|
| bash | `~/.local/share/bash-completion/completions/aisw` |
| zsh | Writable `fpath` entry, or `~/.zsh/completions/_aisw` |
| fish | `~/.config/fish/completions/aisw.fish` |

### Manual install from source

```sh
cargo build --release

install -Dm644 completions/aisw.bash \
  ~/.local/share/bash-completion/completions/aisw

install -Dm644 completions/_aisw \
  ~/.zsh/completions/_aisw

install -Dm644 completions/aisw.fish \
  ~/.config/fish/completions/aisw.fish
```

For zsh, ensure the completion directory is in your `fpath`:

```zsh
fpath=(~/.zsh/completions $fpath)
autoload -U compinit && compinit
```
