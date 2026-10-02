---
title: VS Code integration RFC
description: RFC for a VS Code extension that provides aisw profile and context switching in VS Code and Cursor.
---

# RFC: VS Code integration

**Status:** Accepted. Implementation in progress (#255).

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
- Windows in the MVP. The CLI runs on Windows, but binary detection, the
  install flow and terminal commands are designed for macOS and Linux only.

## Design goals and guidance

The extension exists for one moment: a developer is about to launch an agent
in a repository and needs to know which account will authenticate. The
design goal is that the answer is always visible, and fixing a wrong answer
takes one click.

**Visual specification:** the
[AISW for VS Code interface spec](https://burakdede.github.io/aisw/design/vscode-extension-spec.html)
shows every state below as a VS Code mockup in Light Modern and Dark Modern,
with an interactive status bar and the CLI field behind each state. This RFC
is the source of truth; the spec page illustrates it. When the two disagree,
fix the spec page to match this RFC.

### Goals

- **Ambient state.** One status bar item answers "which account is active
  here?" without opening anything. Hovering shows every tool's profile.
- **Native, not custom.** Use only VS Code's own surfaces and codicons, so the
  extension behaves like the rest of the editor in VS Code and Cursor alike.
  No webview, no custom icon font, no custom colors. The AISW brand appears
  only in the Marketplace listing and the walkthrough.
- **One action per switch.** Switching a context or profile is one Quick Pick
  selection, or one notification button when the workspace expects a
  different context.
- **Every failure is actionable.** Each problem state names its cause and
  offers the fix: a command to run, a setting to open, or a page to read.
- **Testable from JSON.** Every state is a pure function of CLI output, so
  the whole UI can be checked against fixtures without launching VS Code.

### Principles

Use these to settle design questions the tables below don't cover. When a
proposal breaks one, change the proposal.

1. **The CLI is the authority.** Every label comes from a JSON field named in
   this RFC. The extension never infers state the CLI did not report.
2. **Color means risk.** The status bar item gets a background color only
   when the wrong account could authenticate in this workspace.
3. **One click per switch, never zero.** Refreshes may run on their own;
   account changes only follow a user action.
4. **Success is quiet, problems are specific.** Confirmations are transient
   status bar messages. Notifications are for problems, and each names the
   cause and offers the fix as a button.

### Writing

- Name things the way users do: "profile", "context", "workspace", and tool
  names as products (Claude Code, Codex CLI, Gemini CLI, Antigravity), never
  internal keys like `agy`.
- Start each message with the problem, then state the fix. No apologies.
- Buttons are Title Case verbs that say what happens: "Switch to
  client-acme", "Show Output", "Undo".
- Quote CLI commands exactly, so a user can copy them into a terminal.

### Review checklist

Before a UI change merges:

- It uses a state, message and icon defined in this RFC, or updates the RFC
  in the same pull request.
- The PR includes screenshots of the affected states in VS Code, in light and
  dark themes. Cursor screenshots are required before a release.
- No new notification for a success path.
- No background color except for the workspace-mismatch states.

## User experience

The extension uses native VS Code surfaces only: one status bar item, Quick
Picks, notifications, a log output channel, the integrated terminal, and a
walkthrough. There is no webview.

### Status bar item

One item on the left (primary) side, because account state applies to the
whole window. `name` is `AISW Account`. The label is the context name when one
matches exactly; otherwise it is the profiles that are actually active.

| State | Text | Background | Driven by | Click |
|---|---|---|---|---|
| Loading | `$(sync~spin) AISW` | none | before the handshake completes | none |
| Context exact | `$(account) client-acme` | none | `status --context`: `context.status == "exact"`, `context.active` | `aisw.switchContext` |
| Context drift | `$(account) acme · drift` | none | `context.status == "drift"` with one `drift_candidates` entry; with several, use the profile label | `aisw.switchContext` |
| Ambiguous | `$(account) work · 2 contexts` | none | `context.status == "ambiguous"`, first of `context.matches` | `aisw.switchContext` |
| No context | `$(account) claude:work +2` | none | `context.status == "none"`; first tool in `tools[]` with an active profile, `+n` other tools with one | `aisw.switchProfile` |
| Nothing active | `$(account) No active profile` | none | profiles exist, every `active_profile` is null | `aisw.switchProfile` |
| First run | `$(account) AISW: Set Up` | none | `list` has no profiles | `aisw.getStarted` |
| Workspace mismatch | `$(warning) personal ≠ client-acme` | `statusBarItem.warningBackground` | `workspace status`: `mismatch`, `unmanaged` (left side `none`), `ambiguous_active`; right side `expected_context` | `aisw.switchToWorkspaceContext` |
| Binding broken | `$(warning) <normal label>` | none | `workspace status`: `invalid_context` | `aisw.switchContext` |
| Switching | `$(sync~spin) Switching to client-acme…` | none | a mutation is in flight | none |
| Not installed | `$(cloud-download) Install aisw` | none | no binary at any step of [binary detection](#installing-the-cli) | `aisw.installCli` |
| Installing | `$(sync~spin) Installing aisw…` | none | an install or update command is running in the AISW terminal | shows the terminal |
| Setting points nowhere | `$(error) AISW: Check binaryPath` | none | `aisw.binaryPath` is set, but the file is missing or not executable | opens the setting |
| Incompatible CLI | `$(error) AISW: Update aisw` | none | unsupported API/schema version or missing feature | `aisw.installCli` in update mode when too old; `aisw.diagnose` when too new |
| Remote window | `$(circle-slash) AISW` | none | `vscode.env.remoteName` is set | tooltip only |
| CLI error | `$(error) AISW: Error` | none | any other read failure | `aisw.showOutput` |

The tooltip is a `MarkdownString` with `supportThemeIcons` and
`isTrusted: { enabledCommands: [...] }` limited to AISW command IDs. It lists
all four tools in `tools[]` order (em dash when no profile is active), a
workspace line from `workspace status` (`matched_rule` on a match,
`expected_context` on a mismatch), and links to Switch Context, Switch Profile
and Verify. On drift it shows the candidate's profile next to the active one
for each tool that differs. `accessibilityInformation.label` reads the state
as one sentence, for example "AISW account: context client-acme, matches this
workspace."

### Installing the CLI

Most people meet the extension right after installing it from the
Marketplace, when `aisw` is usually missing. The extension never bundles or
downloads the binary itself (ADR-0001). It finds an existing install, or runs
the official installer the user picks in a visible terminal, then picks up the
result without a restart.

Guidance for this flow:

- **Find before asking.** Many "missing" binaries are installed, just not on
  the PATH VS Code started with.
- **Install in plain sight.** Install commands run in the AISW terminal, where
  the user can read exactly what runs. The extension only starts the command
  the user chose.
- **Offer what works here.** Methods are filtered by operating system and by
  the tools found on the machine.
- **No restart.** When the install command ends, the extension finds the new
  binary itself.

**Binary detection**, in order:

| Order | Where | If found |
|---|---|---|
| 1 | `aisw.binaryPath`, when set | Used as is. If the file is missing or not executable, show the "setting points nowhere" state. No fallback, so a wrong setting is never hidden. |
| 2 | `aisw` on the extension host's PATH | Used as is. |
| 3 | `~/.local/bin/aisw` (install script); `/opt/homebrew/bin/aisw`, `/usr/local/bin/aisw`, `/home/linuxbrew/.linuxbrew/bin/aisw` (Homebrew); `~/.cargo/bin/aisw` (Cargo) | Used for this session. The tooltip and diagnostics show the path; nothing is written to settings. |
| 4 | Nothing found | "Not installed" state. |

Step 3 exists because VS Code reads the login shell's PATH once at startup,
and the install script's directory is often not on it. It checks only these
fixed absolute paths, never a path inside the workspace. Every candidate must
pass the `version --json` handshake before use. Detection runs again when an
install command ends, when the AISW terminal closes, when `aisw.binaryPath`
changes, and on window focus while aisw is missing.

**First run.** VS Code opens a new extension's walkthrough on install
(`workbench.welcomePage.walkthroughs.openOnInstall`, default `true`), so the
walkthrough's install step is the first surface most users see. When aisw is
missing and the walkthrough isn't open, an information notification (not an
error) appears once per session: "AISW needs the aisw command-line tool to
switch your accounts." with Install aisw and Locate aisw….

**Install picker** (`aisw.installCli`), a Quick Pick titled "Install aisw" with
the placeholder "Choose how to install aisw on macOS" (or Linux). Each method shows its
exact command as the description and a one-line detail. Selecting one runs it
in the AISW terminal straight away.

| Method | Shown when | Command |
|---|---|---|
| Homebrew (focused first) | `brew` is found | `brew install burakdede/tap/aisw` |
| Install script | always | `sh -c "curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh \| sh"` |
| Cargo | `cargo` is found | `cargo install aisw --locked` |
| Locate an Existing aisw… | always | `aisw.locateCli` |
| Open Installation Guide | always | opens the install docs |

`brew` and `cargo` are found with the same PATH-then-known-locations lookup as
aisw. Commands run through `shellIntegration.executeCommand(executable, args)`,
so no shell quoting is involved.

**Locate aisw** (`aisw.locateCli`) uses `showOpenDialog` with
`openLabel: "Use This aisw"`. The file must pass
the handshake before it is saved to `aisw.binaryPath` in user settings.
Otherwise: "That file isn't a compatible aisw: <reason>."

**After the install command:**

- **Exit 0 and aisw found:** "aisw 0.3.10 is installed. Import the accounts
  you're already signed into?" with Import Logins and Add Profile Manually.
  Import Logins runs `aisw init --no-shell-hook` in the AISW terminal; aisw
  detects existing logins and asks before importing each one, and shell files
  stay untouched. The imported profiles then appear in the status bar.
- **Found off PATH:** the extension uses the binary anyway, and the tooltip
  says "Using ~/.local/bin/aisw, which isn't on VS Code's PATH." Diagnostics
  suggest adding the directory to PATH.
- **Non-zero exit:** "Installing aisw with Homebrew failed (exit code 1). The
  terminal shows what happened." with Try Another Method (the picker without
  the failed method) and Show Terminal. Exit 0 with no aisw found afterwards
  uses the same notification, with "finished, but aisw wasn't found".
- **No shell integration:** "Finish the install in the terminal, then check
  again." with Check Again, shown right after the command is sent. Detection
  also runs again when the terminal closes or the window regains focus.

**Updating.** When aisw is too old, the same picker opens as "Update aisw",
with the placeholder "aisw 0.2.4 is installed; this extension needs 0.3.10 or
later". The method that installed the current binary comes first, judged from
its resolved path: a Homebrew prefix or `Cellar` gives `brew upgrade aisw`,
`~/.cargo/bin` gives `cargo install aisw --locked --force`, and `~/.local/bin`
reruns the install script.

**Setting points nowhere.** "aisw.binaryPath points to <path>, but there's no
file there." with Clear Setting (removes the user setting and runs detection
again) and Open Setting.

In remote windows the install flow is not offered; the remote state applies.

### Switching

Both pickers use `createQuickPick` so they can show `busy` while refreshing.

- **Switch Context.** The workspace's `expected_context` comes first under a
  "this workspace" separator with the `repo` icon. A `check` marks the active
  context. Descriptions are the mappings from `context list` in `tool:profile`
  form. On drift, the candidate is listed first as "Reapply <name>". The last
  item is "Switch a Single Profile…".
- **Switch Profile.** Grouped by tool with separators (Claude Code, Codex CLI,
  Gemini CLI, Antigravity). The description is the `auth` field from `list`.
  The `agy` key is displayed as Antigravity and invoked as `antigravity`. The
  last item is "Add Profile…".
- **Empty states.** No contexts: "Create a Context in the Terminal…" types
  `aisw context create ` into the AISW terminal without pressing Enter. No
  profiles: "Add Profile…" and "Get Started with AISW".

A selected row runs exactly one mutation (`use` or `context use`, with
`--json`), then refreshes, then verifies only the tools in
`result.affected_tools`. A clean result shows a 5 s status bar message,
"$(check) Switched to client-acme · 3 tools verified". Verification issues or
non-empty `result.warnings` show a warning notification instead.

### Notifications

| Situation | Severity | Message | Buttons |
|---|---|---|---|
| Workspace mismatch | warning | This workspace expects the client-acme context, but personal is active. | Switch to client-acme · Not Now · Don't Ask for This Workspace |
| Switched, verify issues | warning | Switched to client-acme, but Codex CLI didn't verify: tool binary not found on PATH. | Run aisw doctor · Show Output |
| Switch failed (`ok: false`) | error | Couldn't switch to client-acme: <first line of `error.message`>. Nothing was changed. | remediation command · Show Output |
| aisw not installed | info | AISW needs the aisw command-line tool to switch your accounts. | Install aisw · Locate aisw… |
| aisw installed | info | aisw 0.3.10 is installed. Import the accounts you're already signed into? | Import Logins · Add Profile Manually |
| Install failed | error | Installing aisw with Homebrew failed (exit code 1). The terminal shows what happened. | Try Another Method · Show Terminal |
| Install exit unknown | info | Finish the install in the terminal, then check again. | Check Again |
| Setting points nowhere | error | aisw.binaryPath points to <path>, but there's no file there. | Clear Setting · Open Setting |
| CLI too old | error | aisw 0.2.4 is too old for this extension. Update aisw to 0.3.10 or later. | Update aisw · Show Output |
| CLI too new | error | This aisw is newer than the extension supports. Update the AISW extension. | Show Output |
| Verify command, issues | warning | 2 tools need attention: Codex CLI (fail), Gemini CLI (warn). | Show Details |
| Profile added | info | Added Claude Code profile work. | Switch to work |
| Terminal flow failed | error | aisw add exited with code 1. The terminal shows what happened. | Show Terminal |
| Terminal exit unknown | warning | Couldn't read the result of aisw add. AISW refreshed its state. | Show Terminal |
| Profile removed | info | Removed Claude Code profile work. | Undo |

Rules:

- The mismatch notification appears at most once per window for each pair of
  active and expected contexts. "Don't Ask for This Workspace" is stored in
  `workspaceState`; `aisw.workspaceMismatch.notify` turns it off. The status
  bar warning stays in both cases.
- Remediation buttons come from `error.remediation` only when
  `kind == "run_command"` and `safe == true`; they open the AISW terminal with
  that command.
- "Nothing was changed" is shown only for `ok: false` envelopes, relying on the
  CLI's transactional rollback.
- `verify` results leave out tools with `stored_profiles == 0`, so a tool the
  user never set up does not raise a warning.
- Not-installed and incompatible notifications appear once per session; the
  status bar keeps the state. The not-installed notification is skipped while
  the walkthrough is open.

### Profile lifecycle

**Add Profile** can involve OAuth or an API key, so it runs in the integrated
terminal and keeps the CLI's native prompts:

1. Quick Pick (step 1/2): tools from `capabilities.tools`, with
   `auth_methods` as the description.
2. Input box (step 2/2): profile name, 1–32 letters, numbers, hyphens or
   underscores, rejected if it already exists for the selected tool.
3. Terminal named `AISW: <tool> profile` with `iconPath: ThemeIcon('account')`,
   `color: ThemeColor('terminal.ansiBlue')` and `isTransient: true`. Wait up to
   3 s for `onDidChangeTerminalShellIntegration`, then run
   `shellIntegration.executeCommand(binary, ['add', tool, name])`. This form
   quotes arguments correctly on every shell.
4. `onDidEndTerminalShellExecution` for that execution: exit 0 refreshes,
   verifies the tool and shows "Added …"; non-zero shows the failure
   notification; `undefined` refreshes and shows the "unknown" warning.
5. Without shell integration, fall back to `sendText` with POSIX shell quoting
   and refresh when the terminal closes. No success notification is shown,
   because no exit code is available.

**Remove Profile** handles no secrets, so it runs in the background:

1. Quick Pick of profiles. The active profile is shown with the description
   "active · switch away first"; selecting it is rejected without invoking the
   CLI.
2. Modal confirmation, `showWarningMessage(..., { modal: true, detail })`:
   "Remove the Claude Code profile work?" with the detail "aisw deletes its
   stored credentials and keeps a backup you can restore." and a Remove button.
3. `aisw remove <tool> <name> --yes --json`, then refresh.
4. "Removed Claude Code profile work." with Undo, which runs
   `aisw backup restore <result.backup_ids[0]> --yes --json`. This round trip
   restores the profile on aisw 0.3.10.

### Commands

All commands declare `category: "AISW"` and a codicon.

| Command | Title | Icon | Enablement |
|---|---|---|---|
| `aisw.switchContext` | Switch Context | `arrow-swap` | `aisw.state == ready` |
| `aisw.switchProfile` | Switch Profile | `account` | `aisw.state == ready` |
| `aisw.switchToWorkspaceContext` | Switch to Workspace Context | `repo` | `aisw.workspaceMismatch` |
| `aisw.verify` | Verify Active Accounts | `pass-filled` | `aisw.state == ready` |
| `aisw.refresh` | Refresh | `refresh` | always (no-op in remote) |
| `aisw.addProfile` | Add Profile… | `add` | `aisw.state == ready` |
| `aisw.removeProfile` | Remove Profile… | `trash` | `aisw.hasProfiles` |
| `aisw.installCli` | Install or Update aisw… | `cloud-download` | `!isRemote` |
| `aisw.locateCli` | Locate aisw… | `folder-opened` | `!isRemote` |
| `aisw.diagnose` | Show CLI Diagnostics | `info` | always (no-op in remote) |
| `aisw.showOutput` | Show Output | `output` | always |
| `aisw.getStarted` | Get Started | `book` | always |

Context keys: `aisw.state` (`loading | ready | cliMissing | installing |
binaryPathInvalid | incompatible | remote | error`), `aisw.cliInstalled`,
`aisw.workspaceMismatch`, `aisw.hasProfiles`, `aisw.hasContexts`. No default
keybindings ship.

### Settings

| Setting | Default | Scope | Notes |
|---|---|---|---|
| `aisw.binaryPath` | `""` (PATH) | `machine` | Must not be settable from workspace settings; see Security. |
| `aisw.workspaceMismatch.notify` | `true` | `window` | Turns off the mismatch notification only. |
| `aisw.refreshDebounceMs` | `500` | `window` | Debounce for focus refreshes. |

### Output channel

`createOutputChannel('AISW', { log: true })`. Each CLI call logs its
subcommand, exit code and duration. Diagnostics print the version and
capability payloads. Arguments are limited to tool, profile and context names;
secrets never appear in arguments.

### Walkthrough

`contributes.walkthroughs` "Get started with AISW". Steps complete on state,
not clicks:

1. Install the aisw CLI: "AISW uses the aisw command-line tool to store and
   switch your accounts. Your credentials never pass through the editor."
   The step invokes `aisw.installCli`; `aisw.locateCli` remains available from
   the command palette. It disappears on `onContext:aisw.cliInstalled`.
2. Import the accounts you're signed into: invokes `aisw.importLogins`, which
   runs `aisw init --no-shell-hook` in the AISW terminal. Add Profile remains
   the alternative. The step disappears on `onContext:aisw.hasProfiles`.
3. Group profiles into a context with the native AISW CLI, then use Show
   Output or Refresh to inspect the result. The step disappears on
   `onContext:aisw.hasContexts`.
4. Bind this repository: invokes `aisw.bindWorkspace`, which runs
   `aisw workspace bind <workspace> --context <name>` in the AISW terminal.

### Assets

- `integrations/vscode/media/icon.png`: 256 × 256 RGBA, cut from
  `website/public/aisw-logo.png` with transparent corners and a 16% navy
  hairline so the white tile reads on light pages. The manifest requires at
  least 128 px; 256 px covers Retina screens.
- `galleryBanner`: `{ "color": "#001436", "theme": "dark" }`, the logo's
  prompt navy.
- Brand colors from the logo: arrow blue `#077FFE`, arrow cyan `#02CDFD`,
  prompt navy `#001436`. They appear only in the Marketplace listing and the
  walkthrough.
- Workbench surfaces use codicons only, following the VS Code status bar
  guidelines. No custom icon font.
- `website/public/design/vscode-extension-spec.html`: the visual
  specification, a self-contained page with inlined codicons and icon. Update
  it in the same pull request as any UI change to this RFC.

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

Module layout keeps every state a pure function of CLI JSON, testable without
VS Code:

```text
src/
  cliAdapter.ts      spawn, envelope and verify-report parsing, timeouts
  contract.ts        payload types, version and feature checks
  state.ts           deriveViewState(status, list, contexts, workspace)
  statusBar.ts       render(ViewState): text, icon, background, tooltip
  pickers.ts         Switch Context, Switch Profile, Add, Remove
  terminalFlows.ts   AISW terminal, shell-integration execution
  notifications.ts   copy and remediation-to-button mapping
  refresh.ts         handshake cache, sequence guard, mutation queue
  extension.ts       activation and wiring only
```

Refresh policy:

- **Activation:** handshake (`version`, `capabilities`) and all reads.
- **Window focus (debounced):** `status --context` and `workspace status`
  only. The handshake is cached per `binaryPath` and rerun only after a
  setting change, an explicit Refresh, or an `unsupported_command` error.
- **After a mutation:** all reads, then scoped verify.
- **Sequence guard:** each refresh takes a sequence number; only the newest
  result is rendered.
- **Mutation queue:** one mutation at a time; focus refreshes are skipped
  while one runs.

The extension must not watch credential files; AISW owns that state and may
use the OS keyring.

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
aisw remove <tool> <profile> --yes --json
aisw backup restore <backup-id> --yes --json
aisw verify --json
```

Use `--non-interactive` whenever the extension owns the process and expects a
machine result. Use the CLI's exit code and structured error envelope to
determine success. AISW remains responsible for rollback.

Contract details the adapter must handle (observed on aisw 0.3.10):

- Read commands print the raw payload; mutations and `project-bindings list`
  print an `{ ok, command, result | error }` envelope.
- `verify --json` prints a report, not an envelope, and exits 1 when
  `summary.status == "fail"`. A tool that is not installed counts as a
  failure. The adapter must parse stdout on a non-zero exit and return the
  report; after a switch, it judges only the `tools[]` entries listed in the
  mutation's `result.affected_tools`.
- `context.status` is one of `exact`, `drift`, `ambiguous`, `none`.
  `workspace status` `status` is one of `match`, `mismatch`, `unmanaged`,
  `invalid_context`, `ambiguous_active`, `no_expected_context`.
- `list` and `status` use the key `agy`; commands accept `antigravity`.
- Mutation results carry `warnings`, which the extension must surface.

`--emit-env` is not an editor state API: it prints shell code and only affects
the process that evaluates it. The extension may open a new terminal with a
selected context, but must not claim that existing terminals or agent sessions
were switched.

## Cursor compatibility

Use only standard VS Code APIs: extension manifest contribution points,
commands, Quick Pick, status bar, notifications, output channel, and terminal.
Do not use `vscode.cursor` APIs unless a concrete Cursor-only requirement is
identified.

`engines.vscode` is `^1.93.0`, the release that finalized the terminal shell
integration API. Pin `@types/vscode` to the same minimum (`~1.93.0`) so the
code cannot compile against newer APIs. Confirm the VS Code base version shown
in Cursor's About dialog during the manual smoke test.

Publish the same package to:

- Visual Studio Marketplace for VS Code.
- Open VSX for Cursor's VSIX-compatible extension installation and other
  compatible editors.

Test the packaged VSIX in both VS Code and Cursor before publishing. The
installed-extension smoke runner accepts `VSCODE_EXECUTABLE_PATH` and
`VSCODE_CLI_PATH` for VS Code, or `CURSOR_EXECUTABLE_PATH` and
`CURSOR_CLI_PATH` for Cursor. For example:

```sh
CURSOR_EXECUTABLE_PATH="/Applications/Cursor.app/Contents/MacOS/Cursor" \
CURSOR_CLI_PATH="/Applications/Cursor.app/Contents/Resources/app/bin/cursor" \
npm run test:integration:cursor
```

The runner creates separate user-data, extensions, `HOME`, and `AISW_HOME`
directories, uses in-memory secret storage, and exercises activation, refresh,
verification, and the native `aisw init --no-shell-hook` terminal flow against
a fake CLI. It does not modify the normal editor install or local AISW state.
Cursor's marketplace and extension availability are not identical to VS
Code's, so distribution is a release concern, not a code fork.

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
- `aisw.binaryPath` is `machine`-scoped and listed in
  `capabilities.untrustedWorkspaces.restrictedConfigurations`. Otherwise a
  cloned repository's `.vscode/settings.json` could point it at a binary
  inside the repository, which the extension would run on window focus.
- Declare `capabilities.virtualWorkspaces: false`.
- The install flow runs only the documented official install commands, and
  only after the user selects one; each runs visibly in the AISW terminal.
  The install script verifies the release binary's SHA-256 checksum. Binary
  detection looks only at fixed absolute locations outside the workspace, and
  every candidate must pass the handshake before use.

For the MVP, a remote extension host shows an explicit unsupported state, and
no command starts a CLI process there. The guard lives in adapter creation, not
only in activation. In a future remote implementation, the CLI and
credentials must run on the same host as the workspace; silently changing the
local machine would be unsafe.

## Repository layout

```text
aisw/
  integrations/
    vscode/
      package.json
      media/icon.png
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
2. Versioned JSON fixtures live with the extension, one directory per status
   bar state above, covering successful reads, mutations, progress, verify
   pass and fail, and stable failures.
3. Extension unit tests run `deriveViewState` and the adapter against a fake
   `aisw` executable built from those fixtures; no credentials or provider
   binaries are required.
4. CI builds the real Rust CLI and runs the extension integration suite against
   that binary for every relevant change. The suite seeds an isolated
   `AISW_HOME` to produce at least the exact, none, mismatch and verify-fail
   states.
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
2. Implement binary detection, the capability handshake, and the install,
   locate and update flows.
3. Fix the adapter contract gaps: verify report parsing, scoped verify, remote
   guard in adapter creation, machine-scoped `aisw.binaryPath`.
4. Implement the status bar states, tooltip, Quick Picks, notifications and
   output channel defined above.
5. Add shell-integration terminal flows for adding profiles and background
   removal with Undo.
6. Add the walkthrough, icon and Marketplace metadata.
7. Add CI coverage against the built CLI and the minimum/latest released CLI.
8. Package and test in VS Code and Cursor, light and dark themes, capturing
   each status bar state; publish the VSIX to the Visual Studio Marketplace
   and Open VSX.
9. If a companion Cursor Plugin is approved, submit its public repository for
   Cursor review and request re-indexing for updates.
10. Reassess remote workspaces and repository extraction after adoption.

## References

- [VS Code Extension API](https://code.visualstudio.com/api/)
- [VS Code extension manifest](https://code.visualstudio.com/api/references/extension-manifest)
- [VS Code workbench extension points](https://code.visualstudio.com/api/extension-capabilities/extending-workbench)
- [VS Code status-bar guidance](https://code.visualstudio.com/api/ux-guidelines/status-bar)
- [VS Code notification guidance](https://code.visualstudio.com/api/ux-guidelines/notifications)
- [VS Code Quick Pick guidance](https://code.visualstudio.com/api/ux-guidelines/quick-picks)
- [VS Code walkthrough guidance](https://code.visualstudio.com/api/ux-guidelines/walkthroughs)
- [Codicon reference](https://microsoft.github.io/vscode-codicons/dist/codicon.html)
- [Cursor extensions](https://prod.cursor.com/help/customization/extensions)
- [Cursor extension API](https://prod.cursor.com/docs/extension-api)
