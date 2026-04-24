# Changelog

## 0.3.3 - 2026-04-24

### Added

- Added `aisw add --from-live` to capture currently active live credentials into a named profile without re-authentication (`b4905f5`, PR #7, contributed by external contributor Julio de Alba, `@juliojair`).
- Added `aisw doctor` diagnostic command for binary detection, keyring availability, shell-hook checks, and permissions health (`9a4af7e`).
- Added `--all --profile <name>` switching so one command can activate same-named profiles across tools (`96e9634`).
- Added TTY-only interactive pickers for missing profile/id arguments in `use`, `remove`, `rename`, and `backup restore` (`858bc83`, `469d7fe`).
- Added filter/sort support for `list`, `status`, and `backup list` (with hardened JSON contracts for script-mode consumers) (`2d6f595`, `c8268e5`).
- Added token-expiry warnings for OAuth-backed profiles in `status` and `use` (`f91df00`).
- Added profile typo suggestions for mistyped names (`82f99c2`).

### Changed

- Gemini OAuth isolation now uses `GEMINI_CLI_HOME` instead of overriding `HOME`, reducing side effects and improving compatibility (`fa2109b`).
- Post-switch output now includes clearer tool/profile/account identity context (`8456def`).
- Claude auth module was split into focused submodules to reduce complexity and improve maintainability (`16aced0`).
- Error handling now uses typed `AiswError` variants with explicit exit-code taxonomy (`d4c1653`).

### Fixed

- Fixed Claude OAuth behavior to preserve live state unless `--set-active`, tolerate capture-dir-ignore behavior, and avoid config-dir override regressions during login (`bb1df80`, `2802581`, `2118be9`).
- Fixed `init` duplicate-import/idempotency issues for Claude OAuth and interactive import paths (`4231f97`, `04b3896`).
- Fixed Gemini OAuth process lifecycle issues (TTY restoration, process-group termination, child cleanup, scratch-dir leak cleanup) (`b477501`, `063f68e`, `fec3a22`, `10b3729`).
- Fixed Claude Keychain apply path to preserve `mcpOAuth` tokens and correctly apply keyring-backed profiles to file-based live auth targets (`d5d63eb`, `e542c00`).
- Fixed Codex config merge to fail safely on malformed TOML (`dd9015b`).

### CI and Quality

- Added native Windows CI coverage (build/unit/integration smoke and secure-backend parity checks) (`6649041`, `c0c89b4`, `50a43b4`, `7f943df`).
- Added coverage quality gates (including branch and critical-path thresholds) and supporting contract/matrix test suites (`22cc674`, `781952b`, `ccf088e`, `dcc1f58`).
- Added Homebrew release automation with checksum mapping hardening on published releases (`fca15c1`, `63b2081`).

## 0.3.2 - 2026-04-01

### Fixed

- Fixed repeated `init` imports and live-account matching for Claude OAuth profiles, including macOS Keychain-backed accounts.
- Clarified `init` output and docs to distinguish live upstream credentials from `aisw`'s recorded active profile state.
- Hardened Linux system-keyring behavior with clearer diagnostics and file-backed fallback when the runtime keyring is unavailable.
- Improved Claude OAuth messaging around browser account selection and duplicate-session confusion.
- Fixed Gemini OAuth capture so it waits for real auth completion, avoids trust-folder setup friction, and reduces terminal corruption after successful login.
- Added terminal restoration so failed or interrupted interactive auth flows do not leave the shell in a broken state.

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
