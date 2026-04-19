# Acceptance Matrix

This matrix records the current end-to-end acceptance status for supported `aisw` auth backends. It is intentionally narrower than the vendor storage inventory in [AUTH_STORAGE_MATRIX.md](../AUTH_STORAGE_MATRIX.md): this document tracks what `aisw` actually supports, how it behaves, and how that behavior is verified.

## Status

| Tool | Live auth/storage situation | `init` import | `use` switch | Expected behavior | Verification |
| --- | --- | --- | --- | --- | --- |
| Claude Code | File-backed credentials | Supported | Supported | Imports `.credentials.json`, stores managed profile metadata/files, applies live credentials file | `tests/init_cmd.rs`, `tests/use_cmd.rs`, full `cargo test` |
| Claude Code | System keyring with readable live entry | Supported | Supported | Imports into managed secure storage, keeps managed secret in system keyring, reapplies live keyring secret on switch | `tests/secure_backend_cmd.rs` integration coverage, full `cargo test` |
| Claude Code | Local state without importable auth | Supported diagnostic | Not applicable | Reports that local Claude state exists but no importable auth was found | `tests/init_cmd.rs`, full `cargo test` |
| Codex CLI | File-backed credentials | Supported | Supported | Imports `auth.json`, keeps `config.toml` aligned to `file`, reapplies live file-backed auth on switch | `tests/init_cmd.rs`, `tests/use_cmd.rs`, full `cargo test` |
| Codex CLI | Managed profile secret stored in system keyring | Not imported from live keyring | Supported | `use` reads the managed Codex profile secret from the system keyring, writes `~/.codex/auth.json`, and keeps `config.toml` aligned to file-backed live auth | `tests/secure_backend_cmd.rs::codex_secure_backend_lifecycle_supports_backup_restore_end_to_end`, `src/commands/status.rs` unit coverage, full `cargo test` |
| Codex CLI | Local state configured for keyring without importable `auth.json` | Supported diagnostic | Not applicable | `init` reports that Codex appears keyring-backed but no importable credential file is available | `tests/init_cmd.rs`, `src/auth/codex.rs` unit coverage, full `cargo test` |
| Gemini CLI | File-managed auth and local state | Supported | Supported | Imports managed Gemini files, preserves required local state files, reapplies live state under `~/.gemini` on switch | `tests/init_cmd.rs`, `tests/use_cmd.rs`, full `cargo test` |
| Gemini CLI | System keyring | Not supported | Not supported | Gemini remains file-managed in `aisw` because upstream behavior is file-centric | Product policy; see [supported-tools.md](./supported-tools.md) |

## Notes

- `Supported diagnostic` means `aisw` can detect and explain the situation without treating it as importable credentials.
- `Fail-closed` means `aisw` intentionally refuses to guess or synthesize a live secure-store identity when doing so could write an unusable or misleading credential entry.
- For secure-backed profiles, `aisw` stores the managed secret in the system keyring rather than downgrading it into `AISW_HOME`.
