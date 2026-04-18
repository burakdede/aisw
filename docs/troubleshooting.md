# Troubleshooting

Targeted fixes for common failures.

## `tool not installed`

Symptom:
- `aisw status` shows a tool as not installed.
- `aisw use` fails for that tool.

Checks:

```sh
which claude
which codex
which gemini
```

Fix:
- install the missing tool
- ensure binary is on `PATH`
- refresh shell cache (`hash -r` for bash, `rehash` for zsh)

## Hook not loaded

Symptom:
- shell-specific env behavior does not update as expected.

Check:

```sh
echo "$AISW_SHELL_HOOK"
```

Fix:

```sh
# zsh
source ~/.zshrc
# bash
source ~/.bashrc
# fish
source ~/.config/fish/config.fish
```

If needed, re-install hook:

```sh
aisw shell-hook zsh >> ~/.zshrc
```

## Non-interactive failures

Symptom:
- command exits in CI with prompt-related error.

Cause:
- `--non-interactive` forbids prompts.

Fix patterns:

```sh
aisw --non-interactive add codex ci --api-key "$OPENAI_API_KEY"
aisw --non-interactive remove codex ci --yes
aisw backup restore <backup_id> --yes
```

## Gemini shared state error

Symptom:

```text
aisw use gemini ... --state-mode shared
```

fails.

Cause:
- Gemini does not support configurable shared state mode in `aisw`.

Fix:
- remove `--state-mode` for Gemini.

## Permission errors

Symptom:
- write/read failures under `~/.aisw` or tool config dirs.

Fix:

```sh
ls -ld ~/.aisw ~/.aisw/profiles
find ~/.aisw -type f -maxdepth 3 -exec ls -l {} \;
```

- ensure your user owns files
- ensure credential files are writable by your user
- re-run `aisw doctor`

## Backup restore did not switch active profile

Expected behavior:
- restore only restores files into profile storage.
- restore does not activate profile.

Use:

```sh
aisw backup restore <backup_id> --yes
aisw use <tool> <profile>
```

## Useful diagnostics

```sh
aisw doctor
aisw status --json
aisw list --json
aisw backup list --json
```

## Still blocked?

Open an issue with:
- command run
- exact error output
- `aisw doctor --json`
- `aisw status --json`

Issues: https://github.com/burakdede/aisw/issues
