---
title: Quickstart
description: Install aisw, store your first profiles, and switch between Claude Code, Codex CLI, and Gemini CLI accounts in under five minutes.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/quickstart.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, quickstart, getting-started
  - tag: meta
    attrs:
      property: article:section
      content: getting-started
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Quickstart","headline":"Quickstart","description":"Install aisw, store your first profiles, and switch between Claude Code, Codex CLI, and Gemini CLI accounts in under five minutes.","url":"https://burakdede.github.io/aisw/quickstart/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, quickstart, getting-started","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.6","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Quickstart","item":"https://burakdede.github.io/aisw/quickstart/"}]}]}
---

From install to switching accounts in five minutes.

## 1. Install

```sh
# Homebrew (macOS and Linux)
brew tap burakdede/tap
brew install aisw

# Shell installer (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh

# Cargo
cargo install aisw
```

Verify:

```sh
aisw --version
```

## 2. Bootstrap

```sh
aisw init
```

This creates `~/.aisw/`, offers to install the optional shell hook (recommended), and detects any accounts you are already logged into. If you are already signed into Claude Code, Codex, or Gemini, `init` will offer to import those credentials as named profiles so you start without re-authenticating.

## 3. Add profiles

**API key:**

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"
```

**From the current environment variable** (useful in CI or when the key is already exported):

```sh
aisw add codex ci --from-env
```

**Interactive OAuth** (opens your browser):

```sh
aisw add claude personal
aisw add codex personal
aisw add gemini personal
```

**Capture the currently logged-in account** (no re-login):

```sh
aisw add claude work --from-live
```

Useful flags:

| Flag | Effect |
|---|---|
| `--label "..."` | Human-readable description shown in `list` and `status` |
| `--set-active` | Activates the profile immediately after adding |

## 4. Switch accounts

Switch a single tool:

```sh
aisw use claude work
aisw use codex personal
aisw use gemini work
```

Switch all tools to the same profile name in one command:

```sh
aisw use --all --profile work
```

When the names stop lining up across tools, save a context instead of forcing fake symmetry:

```sh
aisw context create acme \
  --claude acme-claude \
  --codex acme-codex \
  --gemini acme-gemini

aisw context use acme
```

Rule of thumb:

- `aisw use --all --profile work` is for the simple case where every tool uses the same profile name.
- `aisw context use acme` is for the real case where each tool may need a different account.

**State mode** (Claude Code and Codex CLI only):

```sh
# Isolated: tool reads from a profile-specific config dir (no shared history)
aisw use claude work --state-mode isolated

# Shared: tool reads from its standard config dir (shared history, settings)
aisw use claude work --state-mode shared
```

The default is `isolated`. Use `shared` when you want the tool to behave as if it was never redirected  -  useful for quick one-off usage or when you rely on existing settings or CLAUDE.md files.

## 5. Inspect state

```sh
# Human-readable summary per tool: installed, active profile, backend, live-match status
aisw status
aisw status --context

# Machine-readable (for scripts)
aisw status --json
aisw status --context --json

# List all stored profiles
aisw list
aisw list claude
aisw list --json

# List saved contexts
aisw context list
aisw context list --json
```

## 6. Maintain profiles

```sh
# Rename
aisw rename claude default work

# Remove a profile (a backup is created automatically)
aisw remove codex old --yes

# List backups
aisw backup list

# Restore a backup, then re-activate
aisw backup restore 20260325T114502Z-claude-work --yes
aisw use claude work
```

## 7. Shell hook (optional but recommended)

The shell hook lets `aisw use` and `aisw context use` apply environment variable exports to the current shell session in addition to writing live config files. It also enforces workspace guardrails before each `claude`, `codex`, or `gemini` launch.

```sh
# Zsh
echo 'eval "$(aisw shell-hook zsh)"' >> ~/.zshrc
source ~/.zshrc

# Bash
echo 'eval "$(aisw shell-hook bash)"' >> ~/.bashrc
source ~/.bashrc

# Fish
echo 'aisw shell-hook fish | source' >> ~/.config/fish/config.fish

# PowerShell
Add-Content $PROFILE "`naisw shell-hook pwsh | Out-String | Invoke-Expression"
. $PROFILE
```

## 8. Workspace guardrails (optional, for multi-repo or multi-client work)

If you work on repos that each require a different account, bind them to the right context so you get a warning when the wrong account is active before launching an agent:

```sh
# Bind a repo to the context it should use
cd ~/clients/acme-api
aisw workspace bind . --context client-acme

# Set a fallback for everything else
aisw workspace bind --default --context personal

# Warn on mismatch (default) or block entirely
aisw workspace guard --mode warn
aisw workspace guard --mode strict

# Check what the current directory resolves to
aisw workspace status
```

See [Workspace guardrails](/aisw/workspace/) for the full setup guide.

## Next steps

- [Commands](/aisw/commands/)  -  full syntax for every command
- [Workspace guardrails](/aisw/workspace/)  -  protect repos from wrong-account launches
- [Automation and scripting](/aisw/automation/)  -  CI and non-interactive patterns
- [How it works](/aisw/how-it-works/)  -  credential storage, platform details, design decisions
