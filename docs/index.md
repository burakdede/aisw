# aisw documentation

`aisw` is a local account/profile manager for Claude Code, Codex CLI, and Gemini CLI.

## Install

```sh
brew tap burakdede/tap
brew install aisw
```

Alternative installers:

```sh
curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh
# or
cargo install aisw
```

## Start here

1. [Quickstart](quickstart.md)
2. [Commands](commands.md)
3. [Automation and Scripting](automation.md)
4. [Troubleshooting](troubleshooting.md)

## Command summary

```text
aisw init [--yes]
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--label TEXT] [--set-active]
aisw use <tool> <profile> [--state-mode isolated|shared]
aisw list [tool] [--json]
aisw status [--json]
aisw remove <tool> <profile> [--yes] [--force]
aisw rename <tool> <old> <new>
aisw backup list [--json]
aisw backup restore <backup_id> [--yes]
aisw uninstall [--dry-run] [--remove-data] [--yes]
aisw shell-hook <bash|zsh|fish>
aisw doctor [--json]
```

## Additional references

- [Adding Profiles](adding-profiles.md)
- [Shell Integration](shell-integration.md)
- [Supported Tools](supported-tools.md)
- [Configuration](config.md)
- [Releases](https://github.com/burakdede/aisw/releases)
