# Troubleshooting

Common issues and solutions for `aisw`.

---

## 1. Shell integration not working

If running `aisw use` updates the profile list but doesn't change the active shell environment for a tool (for example `CLAUDE_CONFIG_DIR` or `CODEX_HOME`), the shell hook might not be loaded.

### Diagnosis
Run:
```sh
echo $AISW_SHELL_HOOK
```
If it's empty, the hook is not loaded.

### Solution
1. Verify the hook line is in your RC file (`.zshrc`, `.bashrc`, or `config.fish`).
2. Restart your terminal or source the RC file manually:
   ```sh
   source ~/.zshrc
   ```
3. Check for shell-specific issues in [Shell Integration](shell-integration.md).

---

## 2. "Tool not installed" error

If `aisw status` reports a tool as not installed, `aisw` cannot find the binary on your PATH.

### Solution
1. Verify the tool is installed and its binary is on your PATH.
   ```sh
   which claude
   which codex
   which gemini
   ```
2. If you installed a tool *after* starting your terminal, try `hash -r` (bash) or `rehash` (zsh) to update the binary cache.

---

## 3. Gemini OAuth Capture Fails

Gemini's OAuth flow captures a token cache by overriding the `HOME` directory to a temporary "scratch" location. 

### Symptom
`aisw add gemini` completes the login in the browser, but `aisw` reports:
`Gemini login completed but no credential files were found in the token cache.`

### Solution
- Ensure `aisw` has permission to create and write to the system temporary directory (usually `/tmp` or `$TMPDIR`).
- Try using an API key instead: `aisw add gemini work --api-key <key>`.

---

## 4. Gemini does not support shared state mode

If `aisw use gemini ... --state-mode shared` fails, that is expected.

### Why
Gemini's native `~/.gemini` directory mixes credentials with broader local state such as:
- history
- trusted folders
- project mappings
- settings
- MCP-related config

Because of that, a Gemini "shared" mode would share the whole native Gemini state, not just account credentials.

### What to do instead
- Use Gemini in the default isolated mode.
- Use Claude or Codex with `--state-mode shared` if you need cross-account continuity while keeping one local tool state.

---

## 5. Permission Denied errors

`aisw` strictly enforces `0600` permissions for your security.

### Symptom
Errors when writing to `~/.aisw/` or `~/.claude/`.

### Solution
1. Ensure your user owns the `~/.aisw` directory and its contents.
2. Check if another process or a different version of the tool has locked the credential files.
3. On macOS, ensure your terminal has "Full Disk Access" if you are trying to manage files in protected system directories.

---

## 6. Duplicate Identity Warning

If you get a warning that an account identity already exists under a different profile name, it means `aisw` detected the same email or account ID in the credentials.

### Solution
- Use the existing profile name reported in the warning.
- If you genuinely want a second alias for the same account, you may need to rename or remove the existing profile first.

---

## Need more help?

If you encounter an issue not listed here, please [report it on GitHub](https://github.com/burakdede/aisw/issues).
