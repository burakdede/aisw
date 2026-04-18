# Supported Tools

`aisw` currently supports:

- Claude Code (`claude`)
- Codex CLI (`codex`)
- Gemini CLI (`gemini`)

## Binary detection

`aisw` resolves tools from `PATH` and checks version via `<binary> --version`.

| Tool | Binary | Minimum known-good version |
|---|---|---|
| Claude Code | `claude` | `1.0.0` |
| Codex CLI | `codex` | `1.0.0` |
| Gemini CLI | `gemini` | `0.1.0` |

If binary lookup fails, `aisw status` reports the tool as missing and `aisw use` for that tool is blocked.

## State mode support

| Tool | `--state-mode isolated` | `--state-mode shared` |
|---|---|---|
| Claude Code | supported | supported |
| Codex CLI | supported | supported |
| Gemini CLI | supported | not supported |

Gemini remains isolated-only because auth and broader CLI state are coupled under `~/.gemini`.

## Auth backend support

| Tool | Backend | Import | Use | Notes |
|---|---|---|---|---|
| Claude Code | file credentials | yes | yes | profile stores file state |
| Claude Code | system keyring | yes (when live entry can be read) | yes | profile can remain keyring-backed |
| Codex CLI | file `auth.json` | yes | yes | portable across OSes |
| Codex CLI | system keyring (discoverable account) | yes | yes | uses resolved keyring account |
| Codex CLI | system keyring (not discoverable) | partial | fail-closed | no fabricated keyring account |
| Gemini CLI | file-backed `~/.gemini` state | yes | yes | no keyring mode in `aisw` |

## References

- [Auth Storage Matrix](https://github.com/burakdede/aisw/blob/main/AUTH_STORAGE_MATRIX.md)
- [Acceptance Matrix](https://github.com/burakdede/aisw/blob/main/docs/acceptance-matrix.md)
