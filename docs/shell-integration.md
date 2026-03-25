# Shell Integration

`aisw` provides a shell hook that wraps the `aisw use` command so that environment
variables (API keys, config directory paths) are applied to your **current shell session**
rather than a child process.

## How it works

The hook intercepts `aisw use <tool> <profile>`, runs the real binary with `--emit-env`,
and applies the emitted `export KEY=VALUE` lines to the shell environment. All other
subcommands are passed through unchanged.

---

## Bash

Add to `~/.bashrc` (or `~/.bash_profile`):

```bash
eval "$(aisw shell-hook bash)"
```

## Zsh

Add to `~/.zshrc`:

```zsh
eval "$(aisw shell-hook zsh)"
```

## Fish

Add to `~/.config/fish/config.fish`:

```fish
aisw shell-hook fish | source
```

Fish cannot `eval` POSIX `export KEY=VALUE` syntax, so the fish hook parses each
line with `string replace` / `string split` and applies the values via `set -gx`.

---

## Verifying the hook is active

After sourcing, run:

```sh
echo $AISW_SHELL_HOOK   # should print 1
```

---

## Disabling the hook

Remove or comment out the `eval` / `source` line from your shell config, then start a
new shell session.
