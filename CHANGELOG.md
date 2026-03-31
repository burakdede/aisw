# Changelog

## 0.3.1 - 2026-03-31

### Fixed

- Fixed Claude Keychain import on macOS so `aisw init` correctly detects and imports live logged-in Claude credentials.
- Refined platform-specific auth import policy, including preferring Keychain before file-based Claude credentials on macOS.
- Tightened macOS-sensitive Claude auth tests so they remain deterministic and do not depend on ambient local login state.

## 0.3.0 - 2026-03-31

### Added

- System keyring-backed credential storage support for Claude Code and Codex profiles, including backup, rename, remove, restore, status, and switching coverage.
- First-run `init` import support for Claude Code and Codex credentials discovered in the system keyring.
- End-to-end secure-backend integration tests covering managed keyring profile lifecycles.

### Changed

- Claude and Codex auth capture flows now detect the active storage backend and persist managed credentials using either files or the system keyring as appropriate.
- `list` and `status` now surface credential backend details so file-backed and secure-backend profiles are distinguishable.
- Documentation and acceptance coverage now reflect the secure storage matrix and platform-specific backend behavior.

### Fixed

- Claude Code onboarding on macOS now handles keychain-backed auth instead of assuming a file-backed `.credentials.json` flow.
- Codex Linux and macOS onboarding now imports and reapplies keyring-backed auth instead of failing closed when file-backed credentials are absent.
- OAuth failure cleanup is hardened so interrupted secure-backend flows do not leave stale partially managed profile state behind.

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
