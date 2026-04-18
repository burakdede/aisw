---
title: Shell Integration
description: Shell hook and completion setup for bash, zsh, and fish.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/shell-integration.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, shell integration, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Shell Integration","headline":"Shell Integration","description":"Shell hook and completion setup for bash, zsh, and fish.","url":"https://burakdede.github.io/aisw/shell-integration/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, shell integration, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Shell Integration","item":"https://burakdede.github.io/aisw/shell-integration/"}]}]}
---

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
