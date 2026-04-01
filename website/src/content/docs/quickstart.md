---
title: Quickstart
description: Install aisw, run first-time setup, add profiles, and switch accounts quickly.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/quickstart.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, Quickstart, getting-started, install aisw, quickstart for Claude Code account switching, quickstart for Codex CLI account switching, quickstart for Gemini CLI account switching
  - tag: meta
    attrs:
      property: article:section
      content: getting-started
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Quickstart","headline":"Quickstart","description":"Install aisw, run first-time setup, add profiles, and switch accounts quickly.","url":"https://burakdede.github.io/aisw/quickstart/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, install aisw, quickstart for Claude Code account switching, quickstart for Codex CLI account switching, quickstart for Gemini CLI account switching","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.1","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Quickstart","item":"https://burakdede.github.io/aisw/quickstart/"}]}]}
---

This guide walks through installing `aisw`, running the first-run wizard, and switching
between accounts.

It is the fastest path if you want to:

- install an AI CLI account switcher
- manage multiple Claude Code accounts on one machine
- manage multiple Codex CLI accounts on one machine
- manage multiple Gemini CLI accounts on one machine
- switch between work and personal AI CLI profiles

---

## 1. Install aisw

Install from crates.io:

```sh
cargo install aisw
```

If `aisw` is not available immediately in the same shell session, add `~/.local/bin` to your shell config and reload it:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

If that `PATH` line is already present, `source ~/.zshrc` is enough.

For local development, build from the checked out repository:

```sh
cargo install --path .
```

Or download a pre-built binary from the GitHub Releases page and place it somewhere on your PATH.

---

## 2. Run the first-run wizard

```sh
aisw init
```

The wizard will:

1. Create `~/.aisw/` and write a default `config.json`.
2. Detect your shell and offer to append the shell hook to your RC file.
3. Scan for existing credentials for Claude Code, Codex CLI, and Gemini CLI, and offer
   to import each one with defaults of profile name `default` and label `imported`. You can
   override both during interactive onboarding. Imported live credentials become active by
   default unless aisw is already managing an active profile for that tool. When an import is
   marked active, `aisw` also applies it to the live tool config immediately.
   `aisw init` is checking the current live upstream account for each tool, not listing every
   stored profile in `~/.aisw`, so what it shows may have changed outside `aisw`.

For Claude Code specifically, onboarding is platform-aware: file-backed auth is imported from
the live Claude config directory, and on macOS `aisw` can also import Claude auth from Keychain
when Claude is signed in that way.

Running `aisw init` a second time is safe — the shell hook will not be duplicated, and
existing profiles will not be overwritten.

Successful setup also prints a short next-step hint so you can move directly into `list` or `use`.

### Shell hook

The shell hook is optional. Normal `aisw use` behavior updates the live config locations that
Claude, Codex, and Gemini actually read, so standalone `claude`, `codex`, and `gemini` commands
pick up the selected profile without extra shell steps.

Accept the prompt during `init`, or install the hook manually if you want shell-level
environment exports for advanced or manual workflows:

| Shell | Command |
|-------|---------|
| bash  | `echo 'eval "$(aisw shell-hook bash)"' >> ~/.bashrc` |
| zsh   | `echo 'eval "$(aisw shell-hook zsh)"' >> ~/.zshrc` |
| fish  | `echo 'aisw shell-hook fish | source' >> ~/.config/fish/config.fish` |

After adding the hook, restart your shell or source the file.

---

## 3. Add a profile

```sh
aisw add claude work --api-key sk-ant-api03-...
aisw add codex personal --api-key sk-...
aisw add gemini client --api-key AIza...
```

Use `--label` to add a human-readable description:

```sh
aisw add claude work --api-key sk-ant-api03-... --label "Work subscription"
```

Use `--set-active` to switch to the new profile immediately after adding it:

```sh
aisw add claude work --api-key sk-ant-api03-... --set-active
```

---

## 4. Switch profiles

```sh
aisw use claude work
aisw use codex personal
```

The selected profile is applied directly to the live config location each tool reads. For
manual shell workflows, `--emit-env` is still available:

```sh
eval "$(aisw use claude work --emit-env)"
```

---

## 5. Check status

```sh
aisw status
```

Shows which profile is active for each tool, whether the binary is installed, and the
state of credential files and whether the live tool config matches the configured active
profile. Token validity, quota, and subscription state are not checked. If profiles are stored
for a tool but none is active, `status` reports that explicitly.

---

## 6. List profiles

```sh
aisw list
aisw list claude
aisw list --json
```

---

## 7. Remove a profile

```sh
aisw remove claude old-work
```

A backup of the profile is created before deletion. Use `--force` to remove the currently
active profile, and `--yes` to skip the confirmation prompt.

## Automation note

If you are scripting `aisw` today:

- use `--yes` for `init`, `remove`, and `backup restore`
- use `--api-key` for non-interactive `add`
- use `--json` on `list`, `status`, and `backup list`
- use `use --emit-env` only when you explicitly want raw shell exports

---

## 8. Rename a profile

```sh
aisw rename claude default work
```

Use this when onboarding imported a generic profile name like `default` and you want a
clearer identifier without deleting and recreating the profile.
