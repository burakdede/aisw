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

## Typical search intents this page answers

- Which AI CLI tools does aisw support?
- Does aisw support Claude Code?
- Does aisw support OpenAI Codex CLI?
- Does aisw support Google Gemini CLI?
- Can I manage multiple accounts for Claude, Codex, and Gemini from one CLI?
