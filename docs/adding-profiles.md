# Adding Profiles

This guide explains how to add and store multiple accounts for Claude Code, Codex CLI, and Gemini CLI.

Use it if you want to:

- add a second Claude Code account
- add a second Codex CLI account
- add a second Gemini CLI account
- keep work and personal AI CLI profiles separate
- understand whether each tool uses OAuth, API keys, or local config files

## Claude Code — API key

```
aisw add claude <name> --api-key <key>
```

Stores `{"apiKey": "<key>"}` in `~/.aisw/profiles/claude/<name>/.credentials.json` with `0600` permissions. When you switch to this profile, aisw copies that credentials file into Claude's live config location.

Anthropic's official docs show API key hints beginning with `sk-ant-api03-...`, but do not appear to publish a strict public format specification for Claude Code API keys.

`aisw` does not currently enforce a Claude-specific prefix or minimum length. It only validates that the key is not empty. The `sk-ant-...` examples in this repo are illustrative, not a claimed official format rule.

Official references:

- https://docs.anthropic.com/en/api/admin-api/apikeys/get-api-key
- https://docs.anthropic.com/en/api/admin-api/apikeys/update-api-key

## Claude Code — OAuth (browser login)

```
aisw add claude <name>
```

Spawns Claude's dedicated auth command with `CLAUDE_CONFIG_DIR` set to the profile directory:

```
CLAUDE_CONFIG_DIR=~/.aisw/profiles/claude/<name> claude auth login
```

Claude's OAuth flow still opens a browser window, but `aisw` now uses the auth-specific login path instead of launching a full coding session.

`aisw` waits for Claude to finish the browser login and then continues. If the browser page shows an authentication code, you do not need to paste it into `aisw`. After the browser reports success, close that window and return to the terminal.

- On Linux and Windows, that is typically `.credentials.json` written into `CLAUDE_CONFIG_DIR`.
- On macOS, newer Claude installs may keep auth in Keychain instead. aisw detects that and captures the resulting auth into the profile store once Claude finishes sign-in.

If aisw can resolve the authenticated OAuth account identity from the stored credentials, it prevents creating a second profile alias for the same account. If identity cannot be resolved reliably, the add still succeeds with a warning.

On macOS, aisw now supports both Claude auth storage models: file-backed credentials when Claude writes `.credentials.json`, and Keychain-backed credentials when Claude keeps auth in the `Claude Code-credentials` Keychain item.

## Codex CLI — API key

```
aisw add codex <name> --api-key <key>
```

Writes two files into the profile directory:

- `config.toml` — sets `cli_auth_credentials_store = "file"` so Codex reads from a file instead of the OS keyring
- `auth.json` — stores `{"token": "<key>"}`

Both files are written with `0600` permissions. When you switch to this profile, aisw copies `auth.json` into `~/.codex/` and ensures Codex is configured to read credentials from a file without overwriting unrelated settings in `config.toml`.

OpenAI's official docs document API key authentication and management, but `aisw` does not currently enforce a Codex key prefix or minimum length. It only validates that the key is not empty. The `sk-...` examples in this repo are illustrative, not a claimed official format rule.

Official references:

- https://platform.openai.com/docs/api-reference/
- https://platform.openai.com/docs/api-reference/project-api-keys

## Codex CLI — OAuth

```
aisw add codex <name>
```

Spawns Codex's headless-friendly device-auth login path with `CODEX_HOME` set to the profile directory (with `config.toml` pre-written):

```
CODEX_HOME=~/.aisw/profiles/codex/<name> codex login --device-auth
```

Codex's login flow writes `auth.json` into `CODEX_HOME`. `aisw` polls for the file and registers the profile on success.

If aisw can resolve the authenticated OAuth account identity from the stored credentials, it prevents creating a second profile alias for the same account. If identity cannot be resolved reliably, the add still succeeds with a warning.

## Gemini CLI — API key

```
aisw add gemini <name> --api-key AIza...
```

Writes `GEMINI_API_KEY=<key>` to `.env` in the profile directory with `0600` permissions. When you switch to this profile, aisw copies this file to `~/.gemini/.env` — no shell eval needed.

Google's official docs document using a Gemini API key via the `GEMINI_API_KEY` environment variable, but `aisw` does not currently enforce a Gemini key prefix or minimum length. It only validates that the key is not empty. The `AIza...` example in this repo is illustrative, not a claimed official format rule.

Official references:

- https://ai.google.dev/gemini-api/docs/quickstart
- https://ai.google.dev/gemini-api/docs/api-key

## Gemini CLI — OAuth

```
aisw add gemini <name>
```

Spawns `gemini` with a scratch `HOME` and captures the resulting `~/.gemini/` token cache into the profile.

Gemini CLI does support headless mode via `-p/--prompt`, but Google's official docs say headless mode only works with an already cached credential or env-based auth such as `GEMINI_API_KEY` or Vertex AI variables. There is no documented headless first-time Google-account browser-login flow for a clean `HOME`, so `aisw add gemini <name>` keeps the upstream interactive login behavior for OAuth capture.

If aisw can resolve the authenticated OAuth account identity from the stored credentials, it prevents creating a second profile alias for the same account. If identity cannot be resolved reliably, the add still succeeds with a warning.
