# 0003: Match upstream keyring transports at the live boundary

## Status

Accepted

## Context

Antigravity uses Go keyring, while aisw uses Rust keyring for its own profiles.
A service/account pair does not imply the same native storage representation.
Go's Windows target is `service:account` with a raw UTF-8 blob; Rust defaults to
`account.service` and UTF-16 passwords. Go searches Linux's login collection
before the default alias, while the Rust backend searches across collections
for its target attribute. macOS Go envelopes are not decoded by Go's other
platform backends. Fixtures created entirely through Rust hid these differences.

## Decision

Use an explicit Go-compatible adapter only for live Antigravity credentials.
Windows uses the Go target and raw secret API. Linux accesses Secret Service's
Go collection and service/username attributes directly, using the already
resolved `dbus-secret-service` library as a direct dependency. macOS retains
the existing Keychain transport. Decode recognized macOS envelopes when
applying credentials to Linux or Windows and compare decoded payloads when
transport representations differ.

Keep aisw-managed profile credentials on their existing Rust storage contract.
Do not change the global Rust credential builder or fall back to a Windows
Rust shadow target. Validate adapter names before invoking native APIs.

## Consequences

Existing managed secrets keep their encoding and addresses. Old unintended
Windows live shadow entries are ignored and retained. Native canaries seed Go
representations independently, verify capture and writes, protect a Windows
shadow sentinel, test conflicting Linux collections, and preserve external
Secret Service token rotation. Real provider-session acceptance remains a
separate release validation step.
