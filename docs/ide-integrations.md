---
title: VS Code integration RFC
description: RFC for a VS Code extension that provides aisw profile and context switching in VS Code and Cursor.
---

# RFC: VS Code integration

**Status:** Accepted

## Summary

Build one standard VS Code extension for VS Code and Cursor. The extension is
a thin UI client of the installed `aisw` CLI; it does not implement provider
authentication or manipulate credential files.

Keep the extension in this repository initially, under
`integrations/vscode/`. The CLI contract is still evolving, and a monorepo
allows one pull request to update the CLI, extension, fixtures, and tests
atomically. Revisit a separate `aisw-vscode` repository after the contract and
release cadence have stabilized.

JetBrains support is out of scope for this RFC.

## Goals

- Switch an AISW profile or context from the editor.
- Show the active context and workspace-binding status.
- Work in both VS Code and Cursor without Cursor-specific code.
- Keep credentials and provider-specific behavior inside AISW.
- Detect incompatible CLI versions before performing mutations.
- Make CLI and extension changes testable in the same pull request.

## Non-goals

- Bundling the AISW binary.
- Reading or writing provider credential files from the extension.
- Implementing OAuth or collecting API keys in an editor form.
- Automatically switching accounts when a workspace opens.
- Changing already-running agent processes or terminals.
- Supporting VS Code Web, Codespaces, or remote workspaces in the MVP.

## User experience

The MVP uses native VS Code surfaces:

- A single status-bar item showing the active context or profile.
- `AISW: Switch Context` and `AISW: Switch Profile` commands.
- A Quick Pick with searchable profiles, contexts, and workspace status.
- `AISW: Refresh`, `AISW: Verify Active State`, and `AISW: Diagnose CLI`.
- Notifications for mutations, failures, and workspace mismatches.
- An output channel containing sanitized command results for diagnostics.

Profile creation and OAuth remain terminal flows. `AISW: Add Profile` may open
the integrated terminal and run the native command; the extension must not put
secrets in settings, logs, or command-line arguments.

## Architecture

```text
VS Code / Cursor
      |
      | JSON commands, current workspace as cwd
      v
aisw CLI
      |
      v
profiles, contexts, workspace bindings, native credential stores
```

The extension discovers the executable from `aisw.binaryPath`, defaulting to
`aisw` on `PATH`. It runs commands with the current workspace directory as the
child process working directory so workspace resolution matches the terminal.

The extension should refresh on activation, workspace change, focus regain
with debounce, and after every mutation. It must not watch credential files;
AISW owns that state and may use the OS keyring.

## CLI contract

On activation, run:

```sh
aisw version --json
aisw capabilities --json
```

The extension must reject an unknown `cli_api_version`,
`json_schema_version`, or required feature. It may ignore additive fields. It
must branch on stable fields such as `ok`, `result`, and `error.kind`, never on
human-readable messages.

Read operations:

```sh
aisw status --context --json
aisw list --json
aisw context list --json
aisw workspace status --json
aisw project-bindings list --json
```

Mutations:

```sh
aisw use <tool> <profile> --json
aisw context use <name> --json
aisw verify --json
```

Use `--non-interactive` whenever the extension owns the process and expects a
machine result. Use the CLI's exit code and structured error envelope to
determine success. AISW remains responsible for rollback.

`--emit-env` is not an editor state API: it prints shell code and only affects
the process that evaluates it. The extension may open a new terminal with a
selected context, but must not claim that existing terminals or agent sessions
were switched.

## Cursor compatibility

Use only standard VS Code APIs: extension manifest contribution points,
commands, Quick Pick, status bar, notifications, output channel, and terminal.
Do not use `vscode.cursor` APIs unless a concrete Cursor-only requirement is
identified.

Publish the same package to:

- Visual Studio Marketplace for VS Code.
- Open VSX for Cursor's VSIX-compatible extension installation and other
  compatible editors.

Test the packaged VSIX in both VS Code and Cursor before publishing. Cursor's
marketplace and extension availability are not identical to VS Code's, so
distribution is a release concern, not a code fork.

Cursor also has a separate Plugin Marketplace. It does not publish VSIX files:
submissions are public Git repositories containing a root `plugin.json` or
`.cursor-plugin/plugin.json`, and each submission is manually reviewed by
Cursor. The VS Code extension is therefore not duplicated as a Cursor Plugin
in this RFC. If AISW later needs a Cursor Marketplace listing, it should be a
separate companion plugin with its own manifest, repository layout, validation,
submission, and re-indexing process. A Cursor Marketplace submission is a
release operation, not an automated VSIX publish step.

Extension releases use independent `vscode-vX.Y.Z` tags. The tag version must
match `integrations/vscode/package.json`; CLI releases do not republish the
extension. Before the first release, provision the `aisw` publisher in the
Visual Studio Marketplace and the `aisw` namespace in Open VSX, then add
`OVSX_TOKEN` as a repository secret. Configure the preferred Marketplace
publishing path with `VSCE_AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, and
`AZURE_SUBSCRIPTION_ID` for GitHub OIDC and Microsoft Entra ID; the workflow
falls back to `VSCE_PAT` until the Azure DevOps PAT retirement. The release
workflow uses `npx --no-install` and publishes only the VSIX produced by the
compatibility gate.

## Security and process boundary

- Never read provider credential files or keyring entries.
- Never log command arguments that could contain secrets.
- Do not collect API keys in ordinary editor settings or input fields.
- Do not silently mutate state on workspace open.
- Display sanitized `error.kind`, message, and remediation from AISW.
- Treat missing CLI, unsupported schema, ambiguous context, remote host, and
  workspace mismatch as separate actionable states.
- Refresh and verify after a mutation before showing success.

For the MVP, a remote extension host should show an explicit unsupported-state
message. In a future remote implementation, the CLI and credentials must run
on the same host as the workspace; silently changing the local machine would be
unsafe.

## Repository layout

```text
aisw/
  integrations/
    vscode/
      package.json
      src/
      test/
      fixtures/
  src/
  tests/
  docs/
```

The extension has its own `package.json`, lockfile, TypeScript configuration,
test scripts, and packaging workflow. It must not add extension dependencies to
the Rust CLI's root package configuration.

## Keeping CLI and extension in sync

The monorepo is the synchronization mechanism while the contract is young:

1. A CLI contract change and the corresponding extension change land in one
   pull request.
2. Versioned JSON fixtures live with the extension and cover successful reads,
   mutations, progress, and stable failures.
3. Extension unit tests run against a fake `aisw` executable built from those
   fixtures; no credentials or provider binaries are required.
4. CI builds the real Rust CLI and runs the extension integration suite against
   that binary for every relevant change.
5. CI also tests the extension against the minimum supported released CLI and
   the current CLI build.
6. A breaking CLI contract change increments `cli_api_version` or the relevant
   schema version and updates the extension in the same pull request.

Compatibility rules:

- Additive JSON fields are backward-compatible.
- Removed fields, changed meanings, and changed required commands require a
  version increment.
- The extension declares a minimum AISW version and required capability flags.
- An incompatible installation gets an upgrade message, never a guessed
  fallback.

### When to split the repository

Move `integrations/vscode` to `aisw-vscode` only when at least two of these are
true:

- the CLI machine contract has reached a stable version;
- extension releases need a substantially different cadence;
- extension contributors need independent permissions or issue tracking; or
- extension CI materially slows CLI development.

At extraction time, preserve the same fixture format and run compatibility CI
against released CLI artifacts. The separate repository should still pin a
minimum CLI version, test the latest CLI, and receive a dispatched compatibility
run whenever the CLI publishes a release.

## Delivery plan

1. Add the extension skeleton and fake-CLI contract fixtures.
2. Implement CLI discovery and the capability handshake.
3. Implement status bar, Quick Pick switching, refresh, verify, and output.
4. Add terminal-based profile creation and removal.
5. Add CI coverage against the built CLI and the minimum/latest released CLI.
6. Package and test in VS Code and Cursor; publish the VSIX to the Visual
   Studio Marketplace and Open VSX.
7. If a companion Cursor Plugin is approved, submit its public repository for
   Cursor review and request re-indexing for updates.
8. Reassess remote workspaces and repository extraction after adoption.

## References

- [VS Code Extension API](https://code.visualstudio.com/api/)
- [VS Code extension manifest](https://code.visualstudio.com/api/references/extension-manifest)
- [VS Code workbench extension points](https://code.visualstudio.com/api/extension-capabilities/extending-workbench)
- [VS Code status-bar guidance](https://code.visualstudio.com/api/ux-guidelines/status-bar)
- [Cursor extensions](https://prod.cursor.com/help/customization/extensions)
- [Cursor extension API](https://prod.cursor.com/docs/extension-api)
