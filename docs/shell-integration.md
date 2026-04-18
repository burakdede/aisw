# Shell Integration

The shell hook is optional.

Without the hook, `aisw use` still updates live tool config files directly.

## Install hook

### Bash

Add to `~/.bashrc` (or `~/.bash_profile`):

```bash
eval "$(aisw shell-hook bash)"
```

### Zsh

Add to `~/.zshrc`:

```zsh
eval "$(aisw shell-hook zsh)"
```

### Fish

Add to `~/.config/fish/config.fish`:

```fish
aisw shell-hook fish | source
```

## Verify

```sh
echo "$AISW_SHELL_HOOK"
# expected: 1
```

## What the hook changes

The hook intercepts `aisw use ...`, applies emitted environment variables in the current shell, and passes all other `aisw` commands through unchanged.

## Disable

Remove the hook line from your shell config and open a new shell.

To remove `aisw`-managed hook blocks automatically:

```sh
aisw uninstall --dry-run
aisw uninstall --yes
```

## Completions

`aisw` ships completion files for bash, zsh, and fish.

Installer targets:

- bash: `~/.local/share/bash-completion/completions/aisw`
- zsh: writable `fpath` entry, or fallback `~/.zsh/completions/_aisw`
- fish: `~/.config/fish/completions/aisw.fish`

Manual install from source:

```sh
cargo build --release
install -Dm644 completions/aisw.bash ~/.local/share/bash-completion/completions/aisw
install -Dm644 completions/_aisw ~/.zsh/completions/_aisw
install -Dm644 completions/aisw.fish ~/.config/fish/completions/aisw.fish
```
