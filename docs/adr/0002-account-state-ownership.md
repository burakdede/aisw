# Bound Antigravity switching to authentication state

## Status

Accepted

## Context

Antigravity and Gemini share `~/.gemini`. Antigravity's documented directories
contain global preferences, integrations, updater timestamps and advisory locks.
Copying every file on account activation rewinds unrelated state, creates false
verification failures, and allows Gemini switches to overwrite Antigravity files.
Antigravity OAuth is a shared native-keyring session; `modelProvider` selects its
account provider or the Gemini API-key provider.

## Decision

Antigravity profiles manage the keyring credential and authentication provider
selection. OAuth activation removes `modelProvider`; API-key activation sets it
to `gemini`. Other settings and runtime/global files remain shared. Rollback
snapshots only the credential and the settings file modified by provider selection.
New profiles do not capture the configuration trees. Legacy tree snapshots stay
in existing profiles and backups, but activation does not restore them.

Decode known upstream envelopes for identity and refreshed-token synchronization
while preserving their serialized envelope for the upstream reader. Repair only
recognizable legacy wrapping; reject malformed recognized envelopes. Verification
checks account identity and provider selection, not runtime snapshots or exact
access-token equality after refresh. It does not establish provider token validity.

Gemini excludes Antigravity's documented directories from capture, activation,
verification and recursive rollback, including their copies in legacy profiles.
Gemini imports require its primary OAuth credential file rather than arbitrary
files beneath the shared parent directory. Gemini verification likewise checks
credential identity and provider selection, allowing token rotation and runtime
changes. API-key/OAuth activation updates provider settings transactionally.
Vertex key imports retain the captured provider choice in the managed env file.
Subjects are opaque, case-sensitive identifiers; email labels do not replace
known subject or organization/workspace boundaries.

## Consequences

Account switching preserves updater locks, preferences and integrations and no
longer snapshots thousands of unrelated files. Existing per-account config-tree
restoration intentionally ends. Users wanting different preferences must manage
those independently. Native keyring/file fallback modes that aisw cannot capture
remain outside supported profile storage paths. Provider revocation and running
processes that retain old credentials still require upstream re-authentication or
a new process; serialization repair cannot renew a revoked login.
