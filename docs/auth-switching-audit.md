# Authentication switching audit (#279)

Reviewed 2026-10-14 against `origin/main` at `3557253` and the reported release
`v0.3.10`. This audit covers credential capture, serialization, identity matching,
refresh preservation, activation, rollback, backup restoration, and verification
across all four integrations. It is an offline contract audit with isolated
runtime tests, not a claim that provider-issued sessions never expire or revoke.

## Plan and dependencies

The critical path was upstream contract inspection → failing regressions → fixes
→ full validation. Per-tool documentation and storage-path reviews were
independent of reproduction cases. Release publication is outside this PR.

## Blast radius and repairs

| Boundary | Confirmed defect | Repair and evidence |
|---|---|---|
| Shared macOS reader → Claude/Antigravity | Released captures include the `security -w` output newline; restoring it changes subsequent output into hex. The already merged #252 strips that transport newline. Internal newlines in Claude JSON still survive that fix. | Keep the transport framing fix; normalize recognized credential JSON to one ASCII line and repair known legacy wrapping. The real disposable-keychain test fails before the additional normalization and passes after it, including multiline/Unicode Claude JSON and clean/damaged Antigravity envelopes. |
| Antigravity credential envelopes | JSON-only identity parsing misses `go-keyring-base64:`/`go-keyring-encoded:` credentials; refreshed tokens are discarded and duplicates accepted. | Decode envelopes for identity separately from storage. Both managed backends pass refresh round trips, duplicate rejection, and legacy hex-capture repair. Malformed recognized envelopes fail before live mutation. |
| Antigravity native platform interoperability | Windows Rust targets/UTF-16 blobs differ from Go's service:account/UTF-8 contract; Linux's Rust-target search can select another collection instead of Go's login session. | Use Go's native target/blob/collection contract only for live credentials. Native canaries failed before the repair with a Rust shadow identity instead of the Go identity. Managed aisw entries retain their existing format. |
| Antigravity cross-platform transport | A macOS Go envelope restored literally on Linux/Windows is not decoded by those Go backends. | Decode recognized envelopes at the live write boundary and compare equal decoded payloads across wire formats, including payloads without identity labels. |
| Antigravity activation and verification | Entire configuration/runtime trees are treated as account state; updater timestamp changes produce verification failure and activation overwrites preferences/integrations. | Activate only the shared keyring credential and provider selector. Verify account/provider state, allowing same-account token rotation. Legacy config snapshots are retained but ignored on activation. Regression tests preserve timestamps, preferences and global MCP settings. |
| Antigravity provider transitions | A cached keyring session can be captured as OAuth while the active provider is an API key; old provider settings can also prevent an OAuth switch from selecting account auth. | Reject that OAuth capture and detection claim; OAuth removes `modelProvider`, API key sets `gemini`. Round-trip tests preserve unrelated settings. |
| Gemini → Antigravity | Gemini copies, compares, restores and deletes Antigravity files under their common `.gemini` parent, including old copies in stored profiles. Arbitrary files can also look like OAuth state. | Exclude Antigravity-owned roots before traversal in every Gemini state operation. Require a primary OAuth credential file for capture. Recursive rollback uses the same boundary. Cross-tool capture/switch/status tests fail before the exclusion and pass afterward. |
| Shared refresh identity | Refresh matching reduces identity to its display email, dropping Claude organization and Codex workspace identifiers. | Use structured identities already used by deduplication. Same-email and absent-email scope regressions fail before the repair and pass afterward. Subject identifiers remain case-sensitive, as required by [OpenID Connect](https://openid.net/specs/openid-connect-core-1_0.html#IDToken). Stable subjects take precedence over email labels. This is a credential-integrity change. |
| Claude isolated metadata | Scoped-keyring refresh reads/persists shared `.claude.json` metadata instead of the profile's own metadata. | Select the metadata source by state mode. The regression gives shared and isolated sessions different organizations with the same email and verifies the isolated token refresh retains its own organization. |
| Gemini provider transitions | Persisted `security.auth.selectedType` can keep OAuth selected after an API-key switch; dormant keys can be captured instead of the selected OAuth provider. | Update credentials and provider settings in one transaction; capture the selected provider, preserve Vertex key selection in imported env profiles, and retain unrelated preferences. Provider round-trip and import regressions cover OAuth, Gemini API keys and Vertex keys. |
| Gemini health and capture | Runtime files and rotated tokens produce false verification failures; nested cache files fail capture because destination parents are missing. | Verify the primary credential identity and selected provider rather than runtime snapshots. Capture owned nested files with validated parent paths. Both failure modes have failing-before/passing-after regressions. |
| Gemini encrypted OAuth mode | Current upstream encrypted storage ignores the file that aisw restores, allowing a misleading successful file-profile switch. | Reject file OAuth capture/activation/verification with `GEMINI_FORCE_ENCRYPTED_FILE_STORAGE=true`. API-key profiles remain supported. The rejection regression verifies live file credentials remain unchanged. |

The shared framing defect affects macOS external keyring reads. aisw-owned
secure-store entries (`service=aisw`) use the native keyring reader, and Codex and
Gemini's supported live file paths do not use the `security` CLI reader.
Organization/workspace preservation and `.gemini` ownership are independent
cross-tool findings uncovered by the audit, rather than new instances of the
macOS newline bug.

## Current upstream contracts

| Integration | Contract reviewed | aisw scope |
|---|---|---|
| Claude Code | [Authentication](https://code.claude.com/docs/en/authentication), latest release `v2.1.296`: macOS Keychain, file fallback, configured directory scoping, organization restrictions. | Preserve file/native-Keychain JSON semantics and structured organization identity. Keep existing scoped/legacy mode restrictions. |
| Codex CLI | [Authentication](https://learn.chatgpt.com/docs/auth), [storage implementation at `rust-v0.162.1`](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/login/src/auth/storage.rs): `CODEX_HOME`, file/keyring/auto/ephemeral stores and workspace constraints. | Import readable `auth.json`; managed secure profiles still apply upstream file storage. Native keyring-only/ephemeral import and shared ChatGPT refresh-token switching remain unsupported. Workspace IDs remain part of refresh identity. |
| Gemini CLI | [Authentication](https://geminicli.com/docs/get-started/authentication/), [storage](https://github.com/google-gemini/gemini-cli/blob/v0.64.0/packages/core/src/config/storage.ts), [OAuth implementation](https://github.com/google-gemini/gemini-cli/blob/v0.64.0/packages/core/src/code_assist/oauth2.ts): file OAuth and optional encrypted storage, global config/runtime under `.gemini`. | Manage file OAuth and API-key profiles; preserve Antigravity roots. Unsupported encrypted OAuth is rejected rather than reported as switched. |
| Antigravity CLI | [Installation/auth](https://antigravity.google/docs/cli/install), [settings](https://antigravity.google/docs/settings/), [MCP](https://antigravity.google/docs/mcp/), [updater/Keychain troubleshooting](https://antigravity.google/docs/cli/troubleshooting/), official `1.3.4` macOS artifact and [macOS](https://github.com/zalando/go-keyring/blob/master/keyring_darwin.go), [Windows](https://github.com/zalando/go-keyring/blob/master/keyring_windows.go) and [Linux](https://github.com/zalando/go-keyring/blob/master/keyring_unix.go) keyring implementations. | Shared keyring auth with a provider setting. Global preferences, integrations and runtime files are shared, not account snapshots. Exact service/account and envelope details are implementation evidence, not a promised public configuration API. |

These are reviewed contracts, not newly claimed end-to-end compatibility
baselines. The existing compatibility baseline is not bumped merely because
newer releases were inspected.

## Ownership and migration

[ADR 0002](adr/0002-account-state-ownership.md) records the account-state boundary;
[ADR 0003](adr/0003-upstream-keyring-transports.md) records the native transport boundary.
New Antigravity profiles store credentials and keyring metadata without copying
the configuration trees. Existing profile/backup `app/` and `shared/` content is
retained for review but is not restored during activation. Gemini likewise
ignores Antigravity content in older snapshots.

`use` normalizes recognizable legacy newline/hex captures before native writes.
Windows applies to `gemini:antigravity` as raw UTF-8, leaving the old unintended
Rust `antigravity.gemini` shadow entry untouched. Linux uses Go's login/default
collection lookup, without Rust target attributes. macOS envelopes are decoded
when restoring onto the other platforms.
Opaque unrecognized values are not blindly hex-decoded; malformed recognized
envelopes are rejected. Expired or revoked credentials need a new upstream login
and capture. Serialization repair cannot obtain a new provider token.

Tests that previously demanded global-config copying now assert preservation:
those old expectations described the faulty ownership policy. Rollback and
backup coverage remain, with provider changes and credentials still checked.
Older tests that lowercased opaque subject identifiers now preserve their exact case according to the identity specification. No test is skipped to obtain a green suite; the existing opt-in canaries remain
opt-in. The obsolete directory-map tests were replaced with capture-boundary
and envelope-contract tests.

## Verification

Local results are recorded below. Remote results are tracked by the pull request
that closes #279. Reproductions used
synthetic secrets, temporary HOME/AISW_HOME, fake secure stores, and a disposable
macOS keychain that was never added to the user's search list. No developer
credentials or real Google-account sessions were used.

- `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  `cargo test --locked --no-fail-fast`, `cargo build --release --locked`, completion
  smoke tests, workflow lint, docs build and SEO checks passed locally.
- The disposable-keychain test passed for clean/damaged Antigravity envelopes
  and multiline/Unicode Claude JSON, including non-BMP characters.
- Command-level audit regressions and organization/workspace/subject unit
  regressions passed. Existing opt-in real credential-store canaries were not
  run against developer stores; disposable-host CI results are tracked on the pull request.
- Local nightly coverage totals: 90.40% lines, 82.65% functions, 90.00% regions,
  72.21% branches. Total thresholds passed. The macOS per-file gate stops at
  unchanged `src/commands/remove.rs` (91.67% against 92%). A clean `origin/main`
  run produces the exact same 91.67%, so this is not an introduced coverage loss.
  The required Linux CI coverage gate remains the release gate.
- Remote CI and native-store canary runs are required before release; consult
  the linked pull request checks for the result at the current commit.
- Required Linux/macOS/Windows CI, Linux coverage gates and extension
  compatibility passed. Linux and Windows native-store canaries also passed.
  The hosted macOS native-store canary remains blocked by cross-process
  Keychain authorization in its fixture ([#282](https://github.com/burakdede/aisw/issues/282)).
  Bounded diagnostics report the failure; they do not convert it to success.

## Limits

Local OS: macOS 27.0.1 on Apple Silicon, rather than the reporter's exact 26.6.2
build. Tests establish credential serialization, account boundaries, provider
selection and transactional behavior. They do not authenticate two real Google
accounts with provider-issued tokens, or prove native cross-application ACLs
for every installed upstream binary. Dedicated credential-store canaries cover
native-store lifecycle separately on disposable CI hosts.

The reporter's literal `Status: fail` corresponds to `aisw verify`; the inspected
`status` command prints a descriptive `State`. The independently failing
regressions establish the defects regardless of that wording discrepancy.

The initial suspected Linux kernel-cache defect was not reproduced. The native
rotation test passed before the adapter change, and inspection showed keyring
3.6.3's selected builder creates Secret Service credentials directly despite
the module's cache documentation. The confirmed Linux defect is collection
selection; the conflicting-collection test failed with the shadow identity.
