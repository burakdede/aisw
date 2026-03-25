# Quickstart

This guide walks through installing `aisw`, running the first-run wizard, and switching
between accounts.

---

## 1. Install aisw

Build from source:

```sh
cargo install --path .
```

Or download a pre-built binary from the releases page and place it somewhere on your PATH.

---

## 2. Run the first-run wizard

```sh
aisw init
```

The wizard will:

1. Create `~/.aisw/` and write a default `config.json`.
2. Detect your shell and offer to append the shell hook to your RC file.
3. Scan for existing credentials for Claude Code, Codex CLI, and Gemini CLI, and offer
   to import each one with defaults of profile name `default` and label `imported`. You can
   override both during interactive onboarding. Imported live credentials become active by
   default unless aisw is already managing an active profile for that tool.

Running `aisw init` a second time is safe — the shell hook will not be duplicated, and
existing profiles will not be overwritten.

### Shell hook

The shell hook is required for `aisw use` to apply environment variables to your current
shell session. Accept the prompt during `init`, or install it manually:

| Shell | Command |
|-------|---------|
| bash  | `echo 'eval "$(aisw shell-hook bash)"' >> ~/.bashrc` |
| zsh   | `echo 'eval "$(aisw shell-hook zsh)"' >> ~/.zshrc` |
| fish  | `echo 'aisw shell-hook fish | source' >> ~/.config/fish/config.fish` |

After adding the hook, restart your shell or source the file.

---

## 3. Add a profile

```sh
aisw add claude work --api-key sk-ant-api03-...
aisw add codex personal --api-key sk-...
aisw add gemini client --api-key AIza...
```

Use `--label` to add a human-readable description:

```sh
aisw add claude work --api-key sk-ant-api03-... --label "Work subscription"
```

Use `--set-active` to switch to the new profile immediately after adding it:

```sh
aisw add claude work --api-key sk-ant-api03-... --set-active
```

---

## 4. Switch profiles

```sh
aisw use claude work
aisw use codex personal
```

Environment variables are applied to your shell session via the hook. Without the hook,
`aisw` records the profile as active but warns that the current shell is not using it yet.
Use the `--emit-env` flag and eval the output yourself:

```sh
eval "$(aisw use claude work --emit-env)"
```

---

## 5. Check status

```sh
aisw status
```

Shows which profile is active for each tool, whether the binary is installed, and the
state of credential files. For Claude and Codex, `status` also reports when the current
shell is not actually using the configured active profile yet. If profiles are stored for
a tool but none is active, `status` reports that explicitly.

---

## 6. List profiles

```sh
aisw list
aisw list claude
aisw list --json
```

---

## 7. Remove a profile

```sh
aisw remove claude old-work
```

A backup of the profile is created before deletion. Use `--force` to remove the currently
active profile, and `--yes` to skip the confirmation prompt.

---

## 8. Rename a profile

```sh
aisw rename claude default work
```

Use this when onboarding imported a generic profile name like `default` and you want a
clearer identifier without deleting and recreating the profile.
