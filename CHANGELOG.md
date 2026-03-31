# Changelog

## 0.3.1 - 2026-03-31

### Fixed

- Fixed Claude Keychain import on macOS so `aisw init` correctly detects and imports live logged-in Claude credentials.
- Refined platform-specific auth import policy, including preferring Keychain before file-based Claude credentials on macOS.
- Tightened macOS-sensitive Claude auth tests so they remain deterministic and do not depend on ambient local login state.

## 0.2.0 - 2026-03-30

### Added

- Configurable `shared` versus `isolated` local state mode for Codex and Claude profile switching.
- `aisw uninstall` with `--dry-run`, `--remove-data`, and `--yes` for safe shell-hook and data cleanup.
- Real sourced-shell integration coverage for `bash`, `zsh`, and `fish`.

### Changed

- Gemini is now explicitly documented and enforced as `isolated`-only because its native state mixes credentials with broader local machine state.
- `init` prompts now use `dialoguer` in interactive terminals while preserving non-TTY scripted behavior.
- Tool configuration and tool detection internals were simplified to reduce duplication and keep future changes lower risk.
- Small glue modules were consolidated into clearer ownership boundaries in `output` and `commands::status`.

### Fixed

- Expanded transactional rollback and live-activation coverage for switching flows.
- Added command-level OAuth integration tests for all supported tools.
- Tightened local pre-commit checks to mirror the main CI quality gates more closely.
