---
title: Shell integration
description: Install and configure the aisw shell hook for bash, zsh, and fish. Understand what the hook does and how shell completions work.
---

# Shell integration

The shell hook is optional. Without it, `aisw use` still writes live tool credential files and updates `~/.aisw/config.json`. The hook adds one capability: applying the emitted environment variable exports (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) into the current shell session.

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

## Verify

```sh
echo "$AISW_SHELL_HOOK"
# Expected: 1
```

## What the hook does

The hook wraps the `aisw` function in your shell. When you run `aisw use ...`, the hook:

1. Runs `aisw use ... --emit-env` to write the credential files and print `export VAR=value` lines to stdout.
2. Evals those exports in the current shell, so `CLAUDE_CONFIG_DIR` and `CODEX_HOME` are set immediately.
3. Passes all other `aisw` subcommands through to the binary unchanged.

Without the hook, you can achieve the same effect manually:

```sh
eval "$(aisw use claude work --emit-env)"
```

## Remove

Remove the `eval` line from your shell config and open a new shell.

To remove hook blocks that `aisw init` or `aisw shell-hook` added:

```sh
aisw uninstall --dry-run    # preview
aisw uninstall --yes        # apply
```

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
