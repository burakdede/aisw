# aisw

Manage multiple accounts for Claude Code, Codex CLI, and Gemini CLI. Switch between accounts instantly when you hit a usage quota.

> Work in progress. Not yet ready for production use.

## The problem

AI coding CLI tools have daily or weekly usage quotas. When a quota runs out, work stops. If you have multiple accounts, switching today means manually editing credential files or re-running OAuth flows. There is no unified tool for this.

## What aisw does

aisw stores named credential profiles for each tool and switches between them with a single command. It does not proxy requests, intercept traffic, or make network calls. It manages credential files only.

## Install

Coming in v1.0.0. See the [releases page](https://github.com/burakdede/aisw/releases) when available.

To build from source:

```
cargo install --git https://github.com/burakdede/aisw
```

## Supported tools

| Tool | Binary | Auth methods |
|------|--------|--------------|
| Claude Code | `claude` | OAuth (browser), API key |
| Codex CLI | `codex` | OAuth (ChatGPT), API key |
| Gemini CLI | `gemini` | OAuth (Google), API key |

## License

MIT
