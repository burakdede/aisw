# Adding Profiles

Use `aisw add <tool> <profile>` to create named profiles for Claude Code, Codex CLI, or Gemini CLI.

## Quick syntax

```text
aisw add claude <profile> [--api-key KEY] [--label TEXT] [--set-active]
aisw add codex <profile> [--api-key KEY] [--from-env] [--label TEXT] [--set-active]
aisw add gemini <profile> [--api-key KEY] [--from-env] [--label TEXT] [--set-active]
```

Without `--api-key` or `--from-env`, `aisw add` runs interactive auth.

## API-key flows

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"
```

`--from-env` reads tool-specific env vars:

- Claude: `ANTHROPIC_API_KEY`
- Codex: `OPENAI_API_KEY`
- Gemini: `GEMINI_API_KEY`

Example:

```sh
aisw add codex ci --from-env
```

## OAuth flows

```sh
aisw add claude personal
aisw add codex personal
aisw add gemini personal
```

- Claude: runs the vendor auth login flow in the profile context.
- Codex: runs device-auth login in the profile context.
- Gemini: runs interactive login in an isolated HOME, then captures resulting `~/.gemini` auth state.

## Useful flags

- `--label "..."`: human-readable label
- `--set-active`: switch to the profile immediately after add

Example:

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY" --label "Work" --set-active
```

## Where profile data is stored

```text
~/.aisw/profiles/<tool>/<profile>/
```

Credential files are written with `0600` permissions.

## Duplicate-account behavior

When identity can be resolved from stored auth data, `aisw` prevents creating multiple profile names for the same underlying account.

## Related

- [Quickstart](quickstart.md)
- [Commands](commands.md)
- [Supported Tools](supported-tools.md)
