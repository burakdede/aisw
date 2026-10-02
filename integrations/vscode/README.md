# AISW for VS Code and Cursor

The packaged VSIX is tested in an isolated VS Code installation. To run the
same test against a locally installed Cursor build, provide its executable and
optionally its CLI path:

```sh
CURSOR_EXECUTABLE_PATH="/Applications/Cursor.app/Contents/MacOS/Cursor" \
CURSOR_CLI_PATH="/Applications/Cursor.app/Contents/Resources/app/bin/cursor" \
npm run test:integration:installed
```

The test uses separate user-data, extensions, `HOME`, and `AISW_HOME`
directories. It never modifies the normal VS Code or Cursor installation.

Switch AISW profiles and contexts from VS Code-compatible editors. The
extension is a thin client of the installed `aisw` CLI and never reads or
stores provider credentials.

Set `aisw.binaryPath` only when `aisw` is not available on `PATH`. Profile
creation and OAuth remain native terminal flows.
