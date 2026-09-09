# Acceptance Matrix

This matrix records the current end-to-end acceptance status for supported `aisw` auth backends. It is intentionally narrower than the vendor storage inventory in [AUTH_STORAGE_MATRIX.md](../AUTH_STORAGE_MATRIX.md): this document tracks what `aisw` actually supports, how it behaves, and how that behavior is verified.

## Versioned compatibility baseline

The baseline below records the upstream releases checked against `aisw` `0.3.8` at commit `457b009`, verified on 2026-09-09. The release pins make the audit reproducible; they are not a guarantee that a future upstream release preserves the same storage contracts. Verification is based on the repository's unit and integration tests plus the documented upstream release and authentication behavior; it does not claim that each vendor binary was installed on every listed operating system in CI.

| Agent | Upstream release | OS coverage | Compatibility basis | Result | Sources |
| --- | --- | --- | --- | --- | --- |
| Claude Code | `v2.1.263` | macOS, Linux, Windows | `cargo test --locked`; file credentials and system-keyring paths in the status above | Pass for `aisw`-supported paths; macOS Keychain behavior remains acceptance-matrix-limited | [release](https://github.com/anthropics/claude-code/releases/tag/v2.1.263), [CLI usage](https://docs.anthropic.com/en/docs/claude-code/cli-usage) |
| Codex CLI | `rust-v0.153.4` | macOS, Linux, Windows | `cargo test --locked`; isolated ChatGPT auth, file API keys, and shared-mode guard | Pass for `aisw`-supported paths | [release](https://github.com/openai/codex/releases/tag/rust-v0.153.4) |
| Gemini CLI | `v0.58.0` | macOS, Linux, Windows | `cargo test --locked`; file/API-key/Vertex paths and the documented individual-tier sunset | Pass for supported paths; Google AI Pro, Ultra, and free-tier individual accounts are upstream-sunset | [release](https://github.com/google-gemini/gemini-cli/releases/tag/v0.58.0), [sunset announcement](https://github.com/google-gemini/gemini-cli/discussions/28017) |
| Antigravity CLI | `1.1.27` | macOS, Linux, Windows | `cargo test --locked`; shared OAuth keyring, Gemini API-key environment path, and documented config-root paths | Pass for `aisw`-supported OAuth and API-key paths; API-key switching requires `--emit-env` or shell integration | [release](https://github.com/google-antigravity/antigravity-cli/releases/tag/1.1.27), [authentication docs](https://antigravity.google/docs/cli/install) |

## Status

| Tool | Live auth/storage situation | `init` import | `use` switch | Expected behavior | Verification |
| --- | --- | --- | --- | --- | --- |
| Claude Code | File-backed credentials | Supported | Supported | Imports `.credentials.json`, stores managed profile metadata/files, applies live credentials file | `tests/init_cmd.rs`, `tests/use_cmd.rs`, full `cargo test` |
| Claude Code | System keyring with readable live entry | Supported | Supported | Imports into managed secure storage, keeps managed secret in system keyring, reapplies live keyring secret on switch | `tests/secure_backend_cmd.rs` integration coverage, full `cargo test` |
| Claude Code | Local state without importable auth | Supported diagnostic | Not applicable | Reports that local Claude state exists but no importable auth was found | `tests/init_cmd.rs`, full `cargo test` |
| Codex CLI | File-backed API key credentials | Supported | Supported | Imports `auth.json`, keeps `config.toml` aligned to `file`, reapplies live file-backed auth on switch | `tests/init_cmd.rs`, `tests/use_cmd.rs`, full `cargo test` |
| Codex CLI | ChatGPT-managed auth added directly in isolated profile | Supported | Supported in isolated mode only | Uses one independently authenticated `CODEX_HOME` per profile; shared mode is blocked before live mutation | `tests/use_cmd.rs`, `tests/gui_contract.rs`, full `cargo test` |
| Codex CLI | ChatGPT-managed auth imported with `--from-live` | Supported bootstrap import | Supported in isolated mode only | Preserved for compatibility, but treated as bootstrap-only; user should re-login directly into the isolated profile for durable refresh behavior | `src/commands/add.rs` unit coverage, `tests/use_cmd.rs`, full `cargo test` |
| Codex CLI | Managed profile secret stored in system keyring | Not imported from live keyring | Supported | `use` reads the managed Codex profile secret from the system keyring, writes `~/.codex/auth.json`, and keeps `config.toml` aligned to file-backed live auth | `tests/secure_backend_cmd.rs::codex_secure_backend_lifecycle_supports_backup_restore_end_to_end`, `src/commands/status.rs` unit coverage, full `cargo test` |
| Codex CLI | Local state configured for keyring without importable `auth.json` | Supported diagnostic | Not applicable | `init` reports that Codex appears keyring-backed but no importable credential file is available | `tests/init_cmd.rs`, `src/auth/codex.rs` unit coverage, full `cargo test` |
| Gemini CLI | Enterprise/API-key/Vertex AI file-managed auth and local state | Supported | Supported | Imports managed Gemini files, preserves required local state files, reapplies live state under `~/.gemini` on switch | `tests/init_cmd.rs`, `tests/use_cmd.rs`, full `cargo test` |
| Gemini CLI | Google AI Pro, Ultra, or free-tier individual account | Upstream sunset | Not supported by upstream | Gemini CLI stopped serving these individual-account tiers on June 18, 2026; migrate to Antigravity | [upstream announcement](https://github.com/google-gemini/gemini-cli/discussions/28017) |
| Gemini CLI | System keyring | Not supported | Not supported | Gemini remains file-managed in `aisw` because upstream behavior is file-centric | Product policy; see [supported-tools.md](./supported-tools.md) |
| Antigravity CLI | Shared live OAuth keyring auth plus documented `~/.gemini` config roots | Supported via `add` / `--from-live` | Supported | Restores the shared live OS keyring session plus `~/.gemini/antigravity-cli/` and `~/.gemini/config/`; upstream does not currently document an isolated per-profile auth root | `src/auth/antigravity.rs`, `src/commands/add.rs`, `src/commands/use_.rs`, full `cargo test` |
| Antigravity CLI | Gemini API-key environment auth | Supported via `--api-key` / `--from-env` | Supported with `--emit-env` or shell integration | Stores the key in the selected managed backend, selects `modelProvider: gemini`, and exports `GEMINI_API_KEY` for `agy` | `src/auth/antigravity.rs`, `src/commands/add.rs`, full `cargo test` |

## Notes

- `Supported diagnostic` means `aisw` can detect and explain the situation without treating it as importable credentials.
- `Fail-closed` means `aisw` intentionally refuses to guess or synthesize a live secure-store identity when doing so could write an unusable or misleading credential entry.
- For secure-backed profiles, `aisw` stores the managed secret in the system keyring rather than downgrading it into `AISW_HOME`.
- The real credential-store canary runs the secure-profile lifecycle for Claude, Codex, and Antigravity on macOS, Linux, and Windows. It runs weekly and can be launched manually; its Antigravity coverage also preserves and restores the shared `gemini` / `antigravity` live keyring entry. The workflow deliberately fails when a native credential store cannot be initialized rather than treating that platform as a pass.
