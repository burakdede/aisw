# Automation and scripting

Use this page when you want to run `aisw` safely from scripts, CI, or shell automation.

## Prompt behavior

Current automation-safe usage:

- Use `--non-interactive` to disable all interactive prompts globally. Commands that still require confirmation or interactive auth will fail clearly instead of blocking.
- Use `--quiet` to suppress human-facing presentation output for automation-oriented runs. This does not affect errors, JSON output, `aisw use --emit-env`, or `aisw shell-hook`.
- Use `--yes` for commands that otherwise prompt for confirmation:
  - `aisw init --yes`
  - `aisw remove ... --yes`
  - `aisw backup restore ... --yes`
- Use `aisw add ... --api-key ...` when you need non-interactive profile creation. Without `--api-key`, `aisw add` uses an interactive auth flow and will fail under `--non-interactive`.
- Use `--json` for machine-readable inventory and status output:
  - `aisw list --json`
  - `aisw status --json`
  - `aisw backup list --json`
- `aisw use --emit-env` and `aisw shell-hook` intentionally print raw shell output for scripting and shell integration.

If you need fully non-interactive automation today, combine `--non-interactive` with explicit `--yes` flags where required, prefer API-key-based `add`, and use the JSON output modes above.

## Output contract

`aisw` uses this output model:

- Human-oriented command results and status output go to stdout.
- Errors are printed to stderr and return a non-zero exit code.
- Interactive prompts appear during prompt-driven flows such as `init`, `remove`, and `backup restore` when you do not pass `--yes` or `--non-interactive`.
- `--json` modes are the supported machine-readable interface for scripting.
- `aisw use --emit-env` and `aisw shell-hook` intentionally emit raw shell text to stdout.
- Commands that mutate `~/.aisw/config.json` take an exclusive config lock. If another `aisw` command is already updating config state, the later command waits briefly and then exits with a clear lock-timeout error instead of risking a lost update or partial overwrite.
- If you decline a confirmation prompt, `aisw` exits non-zero with a cancellation error instead of treating the operation as a successful no-op.

Supported JSON interfaces:

- `aisw list --json`
- `aisw status --json`
- `aisw backup list --json`

JSON output is intended to remain stable for automation within a released major version. Human-readable stdout should be treated as presentation output and may change between releases.

`--quiet` behavior:

- suppresses human-oriented presentation output for normal command results
- does not suppress stderr errors
- does not suppress `--json` output
- does not suppress `aisw use --emit-env` or `aisw shell-hook`

## Practical patterns

### Create a profile without prompts

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
```

### Remove a profile non-interactively

```sh
aisw remove claude work --yes
```

### Read profile inventory from a script

```sh
aisw list --json
```

### Export shell variables for the selected profile

```sh
eval "$(aisw use codex work --emit-env)"
```

## Recommended references

- [Commands](commands.md) for the full command and flag reference
- [Shell Integration](shell-integration.md) for hook and completion setup
- [Quickstart](quickstart.md) for the common interactive workflow
