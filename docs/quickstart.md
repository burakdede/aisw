# Quickstart

Minimal path to productive usage.

## 1. Install

```sh
brew tap burakdede/tap
brew install aisw
```

## 2. Run setup

```sh
aisw init
```

What `init` does:
- creates `~/.aisw/`
- offers shell-hook setup
- offers importing currently logged-in accounts for Claude/Codex/Gemini

## 3. Add profiles

```sh
# API key flow
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex personal --api-key "$OPENAI_API_KEY"
aisw add gemini team --api-key "$GEMINI_API_KEY"

# Interactive OAuth flow
aisw add claude personal
aisw add codex work
aisw add gemini personal

# From existing environment variable
aisw add codex ci --from-env
```

Useful flags:
- `--label "..."` add description
- `--set-active` activate immediately

## 4. Switch profiles

```sh
aisw use claude work
aisw use codex personal
aisw use gemini team
```

Batch switch all tools to same profile name:

```sh
aisw use --all --profile work
```

State mode (Claude/Codex only):

```sh
aisw use codex work --state-mode shared
aisw use claude work --state-mode isolated
```

## 5. Inspect state

```sh
aisw status
aisw list
aisw list --json
```

## 6. Common maintenance

```sh
# Rename
aisw rename claude default work

# Remove (backup is automatic)
aisw remove codex old --yes

# Restore backup then re-apply
aisw backup list
aisw backup restore <backup_id> --yes
aisw use codex work
```

## Automation-safe patterns

```sh
aisw --non-interactive add codex ci --api-key "$OPENAI_API_KEY"
aisw --non-interactive remove codex ci --yes
aisw status --json
```

If you only need command syntax, use [Commands](commands.md).
