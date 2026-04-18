---
title: aisw documentation
description: Install and use aisw for account/profile management across Claude Code, Codex CLI, and Gemini CLI.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/index.md
template: splash
hero:
  title: "aisw"
  tagline: "Account manager and switcher for Claude Code, Codex CLI, and Gemini CLI. Current release: v0.3.2."
  actions:
    - text: Quickstart
      link: /aisw/quickstart/
      variant: primary
    - text: Commands
      link: /aisw/commands/
      variant: secondary
    - text: Releases
      link: https://github.com/burakdede/aisw/releases
      variant: minimal
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, aisw documentation, overview
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"WebPage","name":"aisw documentation","headline":"aisw documentation","description":"Install and use aisw for account/profile management across Claude Code, Codex CLI, and Gemini CLI.","url":"https://burakdede.github.io/aisw/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, aisw documentation, overview","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"}]},{"@type":"FAQPage","mainEntity":[{"@type":"Question","name":"What does aisw actually change when I switch accounts?","acceptedAnswer":{"@type":"Answer","text":"aisw use applies the selected profile into the live config location that Claude Code, Codex CLI, or Gemini CLI already reads. It does not patch the tool binary, install a proxy, or change anything outside the relevant local credential and config files."}},{"@type":"Question","name":"Does aisw send credentials or prompts over the network?","acceptedAnswer":{"@type":"Answer","text":"No. aisw itself does not proxy requests, inspect prompts, or send your credentials to a remote service. It is a local credential and profile switcher."}},{"@type":"Question","name":"Where are profiles stored, and how are they protected?","acceptedAnswer":{"@type":"Answer","text":"Stored profiles live under ~/.aisw/profiles/<tool>/<name>/. Credential files are written with 0600 permissions so only your user can read or write them, and aisw status reports files that are broader than that."}}]}]}
---

`aisw` is a local account/profile manager for Claude Code, Codex CLI, and Gemini CLI.

## Install

```sh
brew tap burakdede/tap
brew install aisw
```

Alternative installers:

```sh
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh
# or
cargo install aisw
```

## Start here

1. [Quickstart](/aisw/quickstart/)
2. [Commands](/aisw/commands/)
3. [Automation and Scripting](/aisw/automation/)
4. [Troubleshooting](/aisw/troubleshooting/)

## Command summary

```text
aisw init [--yes]
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--label TEXT] [--set-active]
aisw use <tool> <profile> [--state-mode isolated|shared]
aisw list [tool] [--json]
aisw status [--json]
aisw remove <tool> <profile> [--yes] [--force]
aisw rename <tool> <old> <new>
aisw backup list [--json]
aisw backup restore <backup_id> [--yes]
aisw uninstall [--dry-run] [--remove-data] [--yes]
aisw shell-hook <bash|zsh|fish>
aisw doctor [--json]
```

## Additional references

- [Adding Profiles](/aisw/adding-profiles/)
- [Shell Integration](/aisw/shell-integration/)
- [Supported Tools](/aisw/supported-tools/)
- [Configuration](/aisw/configuration/)
- [Releases](https://github.com/burakdede/aisw/releases)
