---
title: Adding Profiles
description: How to add and capture named profiles in aisw using API keys, OAuth, environment variables, and live credential import.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/adding-profiles.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, adding profiles, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Adding Profiles","headline":"Adding Profiles","description":"How to add and capture named profiles in aisw using API keys, OAuth, environment variables, and live credential import.","url":"https://burakdede.github.io/aisw/adding-profiles/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, adding profiles, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.8","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Adding Profiles","item":"https://burakdede.github.io/aisw/adding-profiles/"}]}]}
---

```text
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--credential-backend file|system-keyring] [--set-active]
```

`<tool>` is one of: `claude`, `codex`, `gemini`.
`<profile>` is any identifier you choose: `work`, `personal`, `client-acme`, `ci`.

## API key

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY"
aisw add codex work --api-key "$OPENAI_API_KEY"
aisw add gemini work --api-key "$GEMINI_API_KEY"
```

## From environment variable

Reads the key from the tool's standard environment variable:

| Tool | Variable |
|---|---|
| Claude | `ANTHROPIC_API_KEY` |
| Codex | `OPENAI_API_KEY` |
| Gemini | `GEMINI_API_KEY` |

```sh
aisw add codex ci --from-env
```

Useful in CI where the key is already exported in the environment.

## Interactive OAuth

Without `--api-key`, `--from-env`, or `--from-live`, `add` launches the tool's native OAuth flow:

```sh
aisw add claude personal
aisw add codex personal
aisw add gemini personal
```

- Claude: spawns `claude auth login`. When the installed Claude build supports profile-scoped auth, `aisw` runs login inside the profile-owned `CLAUDE_CONFIG_DIR`, waits for a non-empty credential payload, and captures account metadata from that directory; otherwise it monitors the live credential file and Keychain for changes and captures the result there.
- Codex: sets `CODEX_HOME` to the profile directory and spawns `codex`. The device-auth flow writes credentials directly into that profile-owned isolated state. This is the durable ChatGPT-managed Codex path.
- Gemini: sets `GEMINI_CLI_HOME` to a scratch directory, spawns `gemini`, then copies the resulting auth/state files into the profile. The scratch directory is removed after the flow regardless of outcome.
- Antigravity: spawns `agy`, captures the resulting live keyring-backed OAuth session plus the documented `~/.gemini/antigravity-cli/` and `~/.gemini/config/` state, then restores the prior live state unless `--set-active` is requested.

Claude OAuth support depends on how the installed Claude build scopes auth:
- File-backed or profile-scoped keychain auth: the interactive login is a durable isolated profile path.
- Legacy shared-Keychain auth: the profile is captured successfully, but `aisw use claude <name> --state-mode shared` is the supported runtime path because `CLAUDE_CONFIG_DIR` does not own the live OAuth credential.
- Unknown keychain behavior: `aisw` will warn that isolated switching may not be durable until the profile is validated on that install.

Interactive OAuth requires a terminal and browser access. It is not available in `--non-interactive` mode.

Important Gemini note: Gemini CLI stopped serving Google AI Pro, Ultra, and free-tier individual accounts on June 18, 2026; those users should migrate to Antigravity. Enterprise Google-account flows and API-key / Vertex AI use remain supported. Some enterprise setups still require `GOOGLE_CLOUD_PROJECT`. For headless or automation use, prefer `GEMINI_API_KEY` or Vertex AI. See the [upstream announcement](https://github.com/google-gemini/gemini-cli/discussions/28017).

## Capture current live credentials

Import what the tool is currently using, without launching a browser:

```sh
aisw add claude work --from-live
aisw add codex work --from-live
aisw add gemini work --from-live
aisw add antigravity work --from-live
```

This is the fastest path if you are already logged in. The captured profile is automatically set as active because those credentials are already live.

For Codex ChatGPT-managed auth, `--from-live` is compatibility/bootstrap only. It captures the current live session, but the durable setup is to re-login directly into the profile with interactive `aisw add codex <name>` so future upstream refreshes stay tied to that profile's own `CODEX_HOME`.

For Codex personal access token sessions, `--from-live` is the current `aisw` path: authenticate upstream with `codex login --with-access-token`, then import that live session. `aisw` treats those profiles separately from ChatGPT-managed refresh-token auth, so the shared-mode ChatGPT block does not apply to them.

For Claude OAuth, `--from-live` captures whatever Claude is currently using, but it does not upgrade a shared live session into an independently isolated auth owner. If the install still uses Claude's legacy shared Keychain credential, treat the imported profile as a captured shared-live session rather than as a durable isolated OAuth bundle.

For Antigravity OAuth, both interactive add and `--from-live` operate on the same shared live upstream model: `aisw` stores the current keyring-backed session and documented Antigravity config roots, then restores them on switch. Upstream does not currently document an isolated per-profile auth root or profile selector.

If a profile with that name already exists, use `--yes` to overwrite it:

```sh
aisw add codex work --from-live --yes
```

## Useful flags

| Flag | Effect |
|---|---|
| `--label TEXT` | Description shown in `aisw list` and `aisw status` |
| `--credential-backend file|system-keyring` | Override where `aisw` stores the managed profile secret |
| `--set-active` | Activates the profile immediately after adding (not needed with `--from-live`, which always activates) |

```sh
aisw add claude work --api-key "$ANTHROPIC_API_KEY" --label "Work account" --set-active
```

## Profile storage

Profiles are stored under:

```text
~/.aisw/profiles/<tool>/<name>/
```

All credential files are written with `0600` permissions. The profile name is recorded in `~/.aisw/config.json` along with the auth method, storage backend, creation timestamp, and label.

`--credential-backend` controls the managed `aisw` profile storage backend, not the upstream CLI's live auth backend.

- `file`: portable and backup-friendly
- `system-keyring`: stronger local secret storage for Claude and Codex where the OS keyring is usable. Stored config and status output use `system_keyring`.
- Gemini remains file-managed because its auth is coupled to broader `~/.gemini/` state
- Antigravity supports `file` and `system-keyring` for the managed profile, but live auth is always restored into Antigravity's shared OS keyring entry.

## Duplicate account detection

When OAuth identity can be resolved from the captured credentials (via JWT claim or OAuth metadata), `aisw` checks whether the same underlying account is already stored under a different profile name. If it is, the `add` command is rejected with an error identifying the existing profile.

This prevents accidentally storing duplicate entries for the same account and having to track which name is the "real" one.

## Related

- [Quickstart](/aisw/quickstart/)
- [Commands](/aisw/commands/)
- [Supported tools](/aisw/supported-tools/)  -  credential locations and backend details per tool
