# AISW for VS Code and Cursor

The packaged VSIX is tested in an isolated VS Code installation. To run the
same test against a locally installed Cursor build, provide its executable and
optionally its CLI path:

```sh
CURSOR_EXECUTABLE_PATH="/Applications/Cursor.app/Contents/MacOS/Cursor" \
CURSOR_CLI_PATH="/Applications/Cursor.app/Contents/Resources/app/bin/cursor" \
npm run test:integration:installed
```

That command uses the VS Code Extension Host test protocol. Cursor does not
implement that protocol in all releases. For a Cursor-specific installed-VSIX
check, use the activation smoke runner:

```sh
CURSOR_EXECUTABLE_PATH="/Applications/Cursor.app/Contents/MacOS/Cursor" \
CURSOR_CLI_PATH="/Applications/Cursor.app/Contents/Resources/app/bin/cursor" \
npm run test:integration:cursor
```

The Cursor runner uses a normal isolated editor window with GPU disabled and
in-memory secret storage. This avoids the host machine's keychain and verifies
exact VSIX installation, extension listing, AISW extension-host activation,
`aisw.refresh`, `aisw.verify`, and the native `aisw init --no-shell-hook`
terminal flow against a fake CLI. It also executes the context and profile
selection paths through the real extension host.
The runner also sets `AISW_EXTENSION_TEST_QUICK_PICK=first` only in its
disposable test environment; normal extension sessions always use the native
Quick Pick UI. Both tests use separate user-data, extensions, `HOME`, and
`AISW_HOME` directories, and never modify the normal VS Code or Cursor
installation. Interactive keyboard selection and cancellation/non-zero
terminal exits still require manual smoke testing in a functioning editor.
Both local harnesses use VS Code 1.93.1 by default, matching the minimum
engine; set `VSCODE_VERSION` to test another VS Code release.

Switch AISW profiles and contexts from VS Code-compatible editors. The
extension is a thin client of the installed `aisw` CLI and never reads or
stores provider credentials.

Set `aisw.binaryPath` only when `aisw` is not available on `PATH`. Profile
creation and OAuth remain native terminal flows.
