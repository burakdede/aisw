---
title: Troubleshooting
description: Diagnosing and fixing common aisw failures  -  missing tools, hook problems, keyring issues, permission errors, and OAuth failures.
---

# Troubleshooting

## Quick diagnostics

Run these first when something is wrong:

```sh
aisw doctor
aisw status --json
```

`doctor` checks binary detection, `~/.aisw/` permissions, shell hook status, and keyring availability. `status --json` shows the full state of every tool including live-match status and any credential warnings.

---

## Tool reported as not installed

**Symptom:** `aisw status` shows a tool as missing, or `aisw use <tool>` fails with "tool not installed".

**Check:**

```sh
which claude
which codex
which gemini
```

**Fix:**
- Install the missing tool (see the vendor's installation instructions).
- Ensure the binary is on your `PATH`.
- Refresh shell binary cache: `hash -r` (bash), `rehash` (zsh).
- If the binary is in a non-standard location, add it to `PATH` before running `aisw`.

---

## Shell hook not active

**Symptom:** `aisw use` applies credentials but environment variables are not updated in the current shell session.

**Check:**

```sh
echo "$AISW_SHELL_HOOK"
# Should print: 1
```

**Fix:**

Reload your shell config:

```sh
source ~/.zshrc    # zsh
source ~/.bashrc   # bash
```

If the hook is not installed, add it:

```sh
aisw shell-hook zsh >> ~/.zshrc && source ~/.zshrc
aisw shell-hook bash >> ~/.bashrc && source ~/.bashrc
```

Note: `aisw use` always writes live credential files regardless of whether the shell hook is active. The hook is only required for shell-level environment variable exports (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`).

---

## Live credentials do not match active profile

**Symptom:** `aisw status` shows "live mismatch" for a tool.

**Causes:**
- You authenticated directly in the tool (not through `aisw`) after the last `aisw use`.
- Another process changed the tool's credential files.
- The profile was stored but never activated with `aisw use`.

**Fix:**

Re-apply the profile:

```sh
aisw use claude work
```

Or capture the current live account as a new profile:

```sh
aisw add claude current --from-live --set-active
```

---

## OAuth flow fails or times out

**Symptom:** `aisw add <tool> <name>` (interactive OAuth) exits with a timeout or credential-not-found error.

**Causes and fixes:**

*Browser did not open or login was not completed:*
- Complete the browser-based login before the timeout.
- If the browser did not open, check that a default browser is configured.

*Tool stores credentials in an unexpected location:*
- Run `aisw doctor` to check for known detection issues.
- File a GitHub issue with the tool version and platform.

*For Gemini  -  scratch directory error:*
- This should not occur in normal usage. If it does, check that `/tmp` is writable.

*For Claude  -  credentials captured but profile creation fails:*
- Check available disk space under `~/.aisw/`.
- Check permissions on `~/.aisw/profiles/`.

---

## Keyring not available (Linux)

**Symptom:** `aisw` reports that the system keyring is unavailable, or keyring-backed operations fail on Linux.

**Cause:** The Secret Service daemon (GNOME Keyring or KWallet) is not running, which is common on headless servers and minimal desktop environments.

**Fix (headless/CI):**

Use `--api-key` or `--from-env` for profiles on Linux servers:

```sh
aisw --non-interactive add codex ci --api-key "$OPENAI_API_KEY"
```

`aisw` automatically falls back to `0600` file-backed storage when the keyring is not available. Run `aisw doctor` to confirm which backend is active.

**Fix (desktop):**

Start the keyring daemon:

```sh
# GNOME
gnome-keyring-daemon --start

# Or ensure the keyring unlocks at login via your desktop environment settings
```

---

## Permission errors

**Symptom:** Read or write failures under `~/.aisw/` or tool config directories.

**Check:**

```sh
ls -ld ~/.aisw ~/.aisw/profiles
find ~/.aisw -type f -maxdepth 3 | xargs ls -l
```

**Fix:**
- Confirm your user owns the files: `ls -la ~/.aisw/`
- Fix ownership if needed: `chown -R $(whoami) ~/.aisw/`
- Fix permissions: `chmod -R u=rwX,go= ~/.aisw/`
- Re-run `aisw doctor` to verify.

---

## Backup restore did not switch the active profile

**Expected behavior:** `aisw backup restore` restores profile files into storage only. It does not activate the profile.

**Fix:** After restoring, explicitly activate the profile:

```sh
aisw backup restore 20260325T114502Z-claude-work --yes
aisw use claude work
```

---

## Non-interactive mode fails in CI

**Symptom:** `aisw` exits with a prompt-related error in a CI environment.

**Cause:** The command requires user input (OAuth flow, overwrite confirmation) but `--non-interactive` is set.

**Fix:**

For API key profiles:

```sh
aisw --non-interactive add claude ci --api-key "$ANTHROPIC_API_KEY"
```

For removals and restores:

```sh
aisw --non-interactive remove codex ci --yes
aisw --non-interactive backup restore <id> --yes
```

Interactive OAuth is not available in `--non-interactive` mode by design. Use API keys or `--from-env` for CI.

---

## `aisw use gemini ... --state-mode shared` fails

**Cause:** Gemini does not support `shared` state mode. Its auth credentials and local state are coupled under `~/.gemini/`, making shared mode unsafe to implement.

**Fix:** Remove `--state-mode` when using Gemini. Gemini profiles are always isolated.

---

## Config lock timeout

**Symptom:** `aisw` reports a lock timeout error.

**Cause:** Another `aisw` command is running concurrently and holds the exclusive config lock.

**Fix:**
- Wait for the other command to complete.
- If no `aisw` process is running, a stale lock may remain. Check for lock files under `~/.aisw/` and remove any that have a modification time older than a minute.

---

## Still blocked?

Run these and include the output when opening an issue:

```sh
aisw doctor --json
aisw status --json
aisw list --json
```

Open an issue at: [github.com/burakdede/aisw/issues](https://github.com/burakdede/aisw/issues)

Include the command you ran, the exact error output, your OS and shell, and the diagnostic output above.
