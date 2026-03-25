# Adding Profiles

## Claude Code — API key

```
aisw add claude <name> --api-key sk-ant-...
```

Stores `{"apiKey": "<key>"}` in `~/.aisw/profiles/claude/<name>/.credentials.json` with `0600` permissions. When you switch to this profile, aisw emits `export ANTHROPIC_API_KEY=<key>` for the shell hook to eval.

API key format: must start with `sk-ant-`, minimum 40 characters.

## Claude Code — OAuth (browser login)

```
aisw add claude <name>
```

Spawns `claude` with `CLAUDE_CONFIG_DIR` set to the profile directory:

```
CLAUDE_CONFIG_DIR=~/.aisw/profiles/claude/<name> claude
```

Claude's OAuth flow opens a browser window. Once you authenticate, Claude writes `.credentials.json` into `CLAUDE_CONFIG_DIR`. aisw polls for this file (every 500ms, up to 120 seconds) and registers the profile once it appears.

**macOS Keychain is never used.** The `CLAUDE_CONFIG_DIR` override causes Claude to store credentials as a plain file instead of in Keychain. This is intentional — it is what makes profiles portable and switchable.

## Codex CLI — API key

```
aisw add codex <name> --api-key <key>
```

Writes two files into the profile directory:

- `config.toml` — sets `cli_auth_credentials_store = "file"` so Codex reads from a file instead of the OS keyring
- `auth.json` — stores `{"token": "<key>"}`

Both files are written with `0600` permissions. When you switch to this profile, aisw sets `CODEX_HOME` to the profile directory.

## Codex CLI — OAuth

```
aisw add codex <name>
```

Spawns `codex` with `CODEX_HOME` set to the profile directory (with `config.toml` pre-written). Codex's login flow writes `auth.json` into `CODEX_HOME`. aisw polls for the file and registers the profile on success.

## Gemini CLI — API key

```
aisw add gemini <name> --api-key AIza...
```

Writes `GEMINI_API_KEY=<key>` to `.env` in the profile directory with `0600` permissions. When you switch to this profile, aisw copies this file to `~/.gemini/.env` — no shell eval needed.

## Gemini CLI — OAuth

```
aisw add gemini <name>
```

Spawns `gemini` with its config directory set to the profile directory. OAuth token files are written there and copied to the active location on switch.
