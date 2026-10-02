# Use the CLI as the editor integration boundary

## Status

Accepted

## Context

AISW users want profile and context switching from VS Code and Cursor. AISW
already exposes versioned JSON, capability discovery, stable errors, progress
events, and workspace-aware commands.

Embedding provider-specific credential logic in each editor would duplicate
security-sensitive behavior and would drift whenever an upstream coding agent
changes its auth storage. A shared editor runtime would also add more coupling
than the first integrations need.

## Decision

Editor integrations invoke the installed `aisw` executable and consume its
machine-readable contract. The CLI remains responsible for credential state,
provider auth, switching, rollback, workspace resolution, and diagnostics.

The first integration is a standard VS Code extension published for VS Code
and Open VSX, which also makes it usable in Cursor. The extension initially
lives in this repository under `integrations/vscode/` so CLI and extension
contract changes can land atomically. A separate repository is a future
extraction option after the contract and release cadence stabilize.

Editor integrations must not bundle the CLI, read provider credential files,
collect secrets in ordinary settings, or switch silently on workspace open.

## Consequences

Positive:

- One security-sensitive implementation remains authoritative.
- The first plugin and CLI can be changed and tested in one pull request.
- VS Code and Cursor share one implementation and one UX.
- CLI improvements automatically become available to integrations after a
  capability check.
- The extension can use native VS Code commands, status bars, notifications,
  and terminals without inventing a cross-editor UI layer.

Costs and constraints:

- Users must install `aisw` separately and keep it on PATH or configure its
  path.
- Plugins need compatibility handling for older or newer CLI contracts.
- Remote development needs an explicit process/credential-location design.
- Marketplace publishing and support remain separate operational concerns.
