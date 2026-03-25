# Supported Tools

aisw manages profiles for the following AI CLI tools.

| Tool | Binary expected on PATH | Minimum version known to work |
|---|---|---|
| Claude Code | `claude` | 1.0.0 |
| Codex CLI | `codex` | 1.0.0 |
| Gemini CLI | `gemini` | 0.1.0 |

aisw detects each tool by searching PATH for the binary name. It does not hardcode install locations. If a binary is not found, `aisw status` reports it as not installed and `aisw use` will refuse to switch to that tool.

Version detection runs `<binary> --version` and captures the output as-is. If the binary exits non-zero or produces no output, the version is reported as unknown — this does not prevent aisw from managing the tool's profiles.
