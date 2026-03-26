---
title: Shell Integration
description: Set up shell hooks, completions, and shell-specific integration for aisw.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/shell-integration.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, Shell Integration, reference, aisw shell hook, aisw zsh completion, aisw bash completion, aisw fish completion
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Shell Integration","headline":"Shell Integration","description":"Set up shell hooks, completions, and shell-specific integration for aisw.","url":"https://burakdede.github.io/aisw/shell-integration/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, aisw shell hook, aisw zsh completion, aisw bash completion, aisw fish completion","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.1.1","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Shell Integration","item":"https://burakdede.github.io/aisw/shell-integration/"}]}]}
---

`aisw` provides an optional shell hook for manual or advanced shell workflows.

## How it works

The hook intercepts `aisw use <tool> <profile>`, runs the real binary with `--emit-env`,
and applies the emitted `export KEY=VALUE` lines to the shell environment. All other
subcommands are passed through unchanged.

Normal `aisw use` behavior no longer depends on this hook. `aisw init` and `aisw use` apply the selected profile directly to the live Claude/Codex/Gemini config locations, so standalone tool invocations pick up the active profile without extra shell steps. The hook is optional.

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

## Tab completion

`aisw` ships completion files for bash, zsh, and fish.

When installed via `install.sh`, completions are installed to these locations:

- Bash: `~/.local/share/bash-completion/completions/aisw`
- Zsh: the first writable directory in `fpath` when detectable, otherwise `~/.zsh/completions/_aisw`
- Fish: `~/.config/fish/completions/aisw.fish`

Manual installation from source:

```sh
cargo build --release

# Bash
install -Dm644 completions/aisw.bash ~/.local/share/bash-completion/completions/aisw

# Zsh
install -Dm644 completions/_aisw ~/.zsh/completions/_aisw

# Fish
install -Dm644 completions/aisw.fish ~/.config/fish/completions/aisw.fish
```

If you use zsh and `~/.zsh/completions` is not already in `fpath`, add this to `~/.zshrc` before `compinit`:

```zsh
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit && compinit
```

To regenerate completion files from source:

```sh
cargo build --release
```

The build writes fresh completion files to `completions/`.

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
