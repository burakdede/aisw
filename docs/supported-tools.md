# Supported Tools

`aisw` supports the main AI coding CLI tools people usually want to manage across multiple accounts: Claude Code, Codex CLI, and Gemini CLI.

If you are looking for a Claude Code account switcher, a Codex CLI profile manager, or a Gemini CLI multi-account workflow, these are the tools currently covered.

| Tool | Binary expected on PATH | Minimum version known to work |
|---|---|---|
| Claude Code | `claude` | 1.0.0 |
| Codex CLI | `codex` | 1.0.0 |
| Gemini CLI | `gemini` | 0.1.0 |

aisw detects each tool by searching PATH for the binary name. It does not hardcode install locations. If a binary is not found, `aisw status` reports it as not installed and `aisw use` will refuse to switch to that tool.

Version detection runs `<binary> --version` and captures the output as-is. If the binary exits non-zero or produces no output, the version is reported as unknown — this does not prevent aisw from managing the tool's profiles.

## State mode support

Claude Code and Codex CLI support configurable switch behavior:
- `isolated`: switch account credentials and local tool state together
- `shared`: keep the tool's local state shared and switch account credentials only

Gemini CLI is currently isolated-only.

Why Gemini differs:
- Gemini stores credentials and broader local state together under `~/.gemini`
- that native directory can include history, trusted folders, project mappings, settings, and MCP-related config
- a Gemini "shared" mode would therefore share the whole native Gemini state, not just auth

Because of that, `aisw` does not currently expose `--state-mode` for Gemini.

## Auth backend support

`aisw` distinguishes between:
- vendor storage behavior: what the upstream CLI stores and where
- `import`: whether `aisw init` can capture an existing live login
- `use`: whether `aisw use` can apply a stored profile back into the live tool safely

The current support policy is:

| Tool | Backend | Import support | Use support | Notes |
|---|---|---|---|---|
| Claude Code | file-backed credentials | supported | supported | uses the live Claude credentials file |
| Claude Code | system keyring | supported where the live entry is readable | supported | managed profiles stay in the system keyring rather than being downgraded to files |
| Codex CLI | file-backed `auth.json` | supported | supported | file-backed profiles remain portable across platforms |
| Codex CLI | system keyring with discoverable live account | supported | supported | `aisw` reuses the existing live keyring account when it can identify it from the live keyring or the stored OAuth identity |
| Codex CLI | system keyring without discoverable live account | partial | fail-closed | `aisw` will not fabricate a username-based keyring account; switching errors with guidance instead |
| Gemini CLI | file-backed local state under `~/.gemini` | supported | supported | Gemini remains file-managed because auth and broader local state are coupled |

Important limits:
- Gemini does not support `system_keyring` profiles in `aisw`
- Codex keyring-backed support is strongest on platforms where the live keyring account can be discovered authoritatively
- Claude Linux and Windows secure-storage behavior is still more weakly documented upstream than Codex

Observed runtime notes:
- On Linux, Claude has been observed storing live auth in `~/.claude/.credentials.json`
- On Linux, Codex has been observed storing live auth in `~/.codex/auth.json`
- On Linux, Gemini has been observed storing OAuth state in `~/.gemini/settings.json` and `~/.gemini/oauth_creds.json`
- Those observations align well with the official docs for Codex and Gemini, but Claude's Linux auth-file location is more strongly runtime-confirmed than vendor-documented

For the vendor storage details behind this policy, see [Auth Storage Matrix](../AUTH_STORAGE_MATRIX.md).
For the current verified end-to-end behavior, see [Acceptance Matrix](./acceptance-matrix.md).

## Typical search intents this page answers

- Which AI CLI tools does aisw support?
- Does aisw support Claude Code?
- Does aisw support OpenAI Codex CLI?
- Does aisw support Google Gemini CLI?
- Can I manage multiple accounts for Claude, Codex, and Gemini from one CLI?
