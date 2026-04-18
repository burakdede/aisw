# Automation and scripting

Use this page for CI or script-safe `aisw` usage.

## Baseline flags

```text
aisw [--non-interactive] [--quiet] <command>
```

- `--non-interactive`: do not prompt; fail instead
- `--quiet`: suppress presentation output (does not suppress errors, JSON, `--emit-env`, or `shell-hook`)
- `--yes`: skip confirmation on commands that prompt

## Non-interactive patterns

```sh
# Add without OAuth/browser flow
aisw --non-interactive add codex ci --api-key "$OPENAI_API_KEY"

# Remove with no prompt
aisw --non-interactive remove codex ci --yes

# Restore backup with no prompt
aisw --non-interactive backup restore <backup_id> --yes
```

## Machine-readable output

Use `--json` for scripts:

```sh
aisw list --json
aisw status --json
aisw backup list --json
```

## Output contract

- human output: stdout
- errors: stderr + non-zero exit code
- prompts: shown only when allowed (no `--non-interactive` and no `--yes`)
- `aisw use --emit-env`: prints shell exports on stdout
- `aisw shell-hook`: prints shell hook code on stdout

## Concurrency

Commands that mutate `~/.aisw/config.json` take an exclusive lock. If another mutating command is already running, the later command times out with a lock error instead of writing partial state.

## Common script snippets

```sh
# Apply profile then run tool command
eval "$(aisw use codex work --emit-env)"

# Check whether expected profile is active
aisw status --json | jq -r '.tools.codex.active_profile'
```

## Related

- [Commands](commands.md)
- [Quickstart](quickstart.md)
- [Shell Integration](shell-integration.md)
