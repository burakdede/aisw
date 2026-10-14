# 0004: Use Go's macOS live Keychain write transport

## Status

Accepted. Supersedes the macOS transport clause of ADR 0003.

## Context

The hosted native canary established that native credential updates replace the
item's partition list with the writer's code hash. Once fixture authorization
allowed activation, Antigravity status hung reading the live item through
`security`. Go's macOS keyring reads and writes through that executable; aisw
mixed its reader with Rust native writes. Serialization tests alone missed the
cross-process access failure (#285).

## Decision

Write live Antigravity credentials through `security -i`, as upstream Go does.
Send the existing normalized wire value through stdin, preserving envelopes.
Use the actual SecurityTool parser's double-quote/backslash rules rather than
POSIX shell quote concatenation. Reject control characters, invalid UTF-8 and
commands above upstream's 4096-byte limit before launching the subprocess.
Capture subprocess output and omit potentially secret-bearing error text.

Keep aisw-managed entries on their native Rust transport. Reauthorize only
synthetic fixture items after native seeding and before independent cleanup;
never grant broader access to user credentials to make a canary pass.

## Consequences

The live writer and upstream reader use the same Keychain application identity.
No global keyring builder or user ACL policy changes. The transport has the same
command-size limit as upstream. The native canary retains its post-activation
status assertion without fixture reauthorization between activation and status.
Real provider-account acceptance is still a separate test.
