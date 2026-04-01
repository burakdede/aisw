# aisw

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-mark-dark.svg">
    <img src="https://raw.githubusercontent.com/burakdede/aisw/main/website/public/aisw-mark-light.svg" alt="aisw logo" width="160" />
  </picture>
</p>

<p align="center">AI and coding agent account manager and account switcher for Claude Code, Codex CLI, and Gemini CLI.</p>

<p align="center">
  <a href="https://crates.io/crates/aisw">
    <img src="https://img.shields.io/crates/v/aisw?style=flat-square" alt="Crates.io version" />
  </a>
  <a href="https://github.com/burakdede/aisw/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/burakdede/aisw/ci.yml?branch=main&style=flat-square&label=CI" alt="CI status" />
  </a>
  <a href="https://github.com/burakdede/aisw/releases">
    <img src="https://img.shields.io/github/v/release/burakdede/aisw?style=flat-square&label=release" alt="Latest release" />
  </a>
  <a href="https://burakdede.github.io/aisw/">
    <img src="https://img.shields.io/badge/docs-website-4c6fff?style=flat-square" alt="Documentation website" />
  </a>
</p>

`aisw` manages multiple accounts for Claude Code, Codex CLI, and Gemini CLI.

It is built for a simple problem: AI coding CLIs make it easy to get blocked by quota limits, account separation, or messy credential state. Switching between work, personal, and backup accounts is usually manual and tool-specific.

`aisw` gives you one workflow for:

- switching to another account when a quota is exhausted
- keeping work and personal accounts separate
- importing the account a tool is already using
- managing profiles across Claude Code, Codex CLI, and Gemini CLI with one CLI

It stores named profiles under `~/.aisw/` and applies the selected profile to the live config each tool actually reads.

## Install

Full install and setup details: [Quickstart](https://burakdede.github.io/aisw/quickstart/).

**curl (Linux, macOS):**

```sh
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh
```

**Cargo:**

```sh
cargo install aisw
```

If `aisw` is not available immediately in the same shell session, refresh `PATH` in that shell:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

If that `PATH` line is already in your shell config, just run `source ~/.zshrc`.

## Quickstart

First time:

```sh
aisw init
```

That detects installed tools, installs shell integration if needed, and offers to import the live account each tool is already using.

Example: switch Claude accounts when one hits quota.

```sh
aisw add claude personal
aisw use claude personal
```

Useful commands:

```sh
aisw add <tool> <name>
aisw use <tool> <name>
aisw list [tool]
aisw status
```

## Shell integration

Normal switching works without shell hooks, but shell integration is available if you want it.

```sh
eval "$(aisw shell-hook bash)"
aisw shell-hook fish | source
```

`aisw init` can add this automatically.

## Supported tools

| Tool | Binary | Auth methods |
|---|---|---|
| Claude Code | `claude` | OAuth (browser), API key |
| Codex CLI | `codex` | OAuth (ChatGPT), API key |
| Gemini CLI | `gemini` | OAuth (Google), API key |

## Docs

- [Quickstart](https://burakdede.github.io/aisw/quickstart/)
- [Commands](https://burakdede.github.io/aisw/commands/)
- [Adding Profiles](https://burakdede.github.io/aisw/adding-profiles/)
- [Shell Integration](https://burakdede.github.io/aisw/shell-integration/)
- [Supported Tools](https://burakdede.github.io/aisw/supported-tools/)

## License

MIT.
