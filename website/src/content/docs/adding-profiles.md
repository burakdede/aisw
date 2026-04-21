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
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, adding profiles, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Adding Profiles","headline":"Adding Profiles","description":"How to add and capture named profiles in aisw using API keys, OAuth, environment variables, and live credential import.","url":"https://burakdede.github.io/aisw/adding-profiles/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, adding profiles, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Adding Profiles","item":"https://burakdede.github.io/aisw/adding-profiles/"}]}]}
---

```text
aisw add <tool> <profile> [--api-key KEY] [--from-env] [--from-live] [--label TEXT] [--set-active]
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

- Claude: spawns `claude auth login`. `aisw` monitors the live credential file and Keychain for changes and captures the result when login completes.
- Codex: sets `CODEX_HOME` to the profile directory and spawns `codex`. The device-auth flow writes credentials directly into the profile.
- Gemini: sets `GEMINI_CLI_HOME` to a scratch directory, spawns `gemini`, then copies the resulting OAuth cache files into the profile. The scratch directory is removed after the flow regardless of outcome.

Interactive OAuth requires a terminal and browser access. It is not available in `--non-interactive` mode.

## Capture current live credentials

Import what the tool is currently using, without launching a browser:

```sh
aisw add claude work --from-live
aisw add codex work --from-live
aisw add gemini work --from-live
```

This is the fastest path if you are already logged in. The captured profile is automatically set as active because those credentials are already live.

If a profile with that name already exists, use `--yes` to overwrite it:

```sh
aisw add codex work --from-live --yes
```

## Useful flags

| Flag | Effect |
|---|---|
| `--label TEXT` | Description shown in `aisw list` and `aisw status` |
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

## Duplicate account detection

When OAuth identity can be resolved from the captured credentials (via JWT claim or OAuth metadata), `aisw` checks whether the same underlying account is already stored under a different profile name. If it is, the `add` command is rejected with an error identifying the existing profile.

This prevents accidentally storing duplicate entries for the same account and having to track which name is the "real" one.

## Related

- [Quickstart](/aisw/quickstart/)
- [Commands](/aisw/commands/)
- [Supported tools](/aisw/supported-tools/)  -  credential locations and backend details per tool
