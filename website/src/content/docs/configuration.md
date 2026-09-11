---
title: Configuration
description: aisw configuration file location, schema, field reference, directory layout, and AISW_HOME override.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/config.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, configuration, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Configuration","headline":"Configuration","description":"aisw configuration file location, schema, field reference, directory layout, and AISW_HOME override.","url":"https://burakdede.github.io/aisw/configuration/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, antigravity cli, account switching, profile manager, credential switching, multiple accounts, work personal accounts, ai coding agent, coding agent account switcher, coding agent profile switch, work personal client profiles, repo account guardrails, anthropic account manager, openai codex account, google gemini cli account, cli tooling, developer tool, configuration, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.10","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Configuration","item":"https://burakdede.github.io/aisw/configuration/"}]}]}
---

## Location

```text
~/.aisw/config.json
```

Override with the `AISW_HOME` environment variable:

```sh
AISW_HOME=/tmp/aisw-test aisw list
```

`AISW_HOME` is useful for isolated testing or for keeping multiple independent `aisw` data directories.

## Permissions

`config.json` is written with `0600` permissions (owner read/write only). `aisw doctor` warns if the file has broader permissions. The `~/.aisw/` directory is created with `0700`.

## Schema

```json
{
  "version": 2,
  "active": {
    "claude": "work",
    "codex": null,
    "gemini": null,
    "antigravity": null
  },
  "profiles": {
    "claude": {
      "work": {
        "added_at": "2026-03-25T10:00:00Z",
        "auth_method": "oauth",
        "credential_backend": "file",
        "label": "Work"
      }
    },
    "codex": {},
    "gemini": {},
    "antigravity": {}
  },
  "contexts": {
    "acme": {
      "profiles": {
        "claude": "acme-claude",
        "codex": "acme-codex",
        "gemini": null,
        "antigravity": null
      },
      "created_at": "2026-03-25T10:05:00Z",
      "updated_at": "2026-03-25T10:05:00Z"
    }
  },
  "settings": {
    "backup_on_switch": true,
    "max_backups": 10,
    "claude": { "state_mode": "isolated" },
    "codex": { "state_mode": "isolated" }
  }
}
```

## Field reference

| Field | Type | Description |
|---|---|---|
| `version` | integer | Config schema version. Commands fail with an upgrade message if this exceeds the installed binary's supported version. |
| `active.<tool>` | string or null | Name of the currently active profile for the tool, or null if none. |
| `profiles.<tool>.<name>` | object | Metadata for a stored profile. Does not contain credential material. |
| `profiles.<tool>.<name>.added_at` | ISO 8601 timestamp | When the profile was created. |
| `profiles.<tool>.<name>.auth_method` | `"oauth"` or `"api_key"` | How the profile was authenticated. |
| `profiles.<tool>.<name>.credential_backend` | `"file"` or `"system_keyring"` | Where the credential bytes are stored. |
| `profiles.<tool>.<name>.label` | string or null | Optional human-readable label. |
| `contexts.<name>` | object | Saved sparse mapping from tool to profile names. |
| `contexts.<name>.profiles.<tool>` | string or null | Profile name to activate for that tool when the context is used. |
| `contexts.<name>.created_at` | ISO 8601 timestamp | When the context was created. |
| `contexts.<name>.updated_at` | ISO 8601 timestamp | When the context was last changed. |
| `settings.backup_on_switch` | boolean | Create a backup before activating a profile. Default: true. |
| `settings.max_backups` | integer | Maximum number of backups to retain. Older ones are pruned when the limit is exceeded. Default: 10. |
| `settings.<tool>.state_mode` | `"isolated"` or `"shared"` | Remembered state mode. Present for `claude` and `codex` only  -  the tools that support it. |

`active`, `profiles`, and each context's `profiles` always carry a key for every supported tool, using `null` where nothing is set.

Credentials are stored under `~/.aisw/profiles/`, not in `config.json`.
Contexts also store only references, never credential material.

## Workspace rules

Workspace bindings live in a separate file, `~/.aisw/workspaces.json`, written by `aisw workspace bind`, `unbind`, and `guard`:

```json
{
  "version": 1,
  "guard_mode": "warn",
  "default_context": null,
  "path_rules": [{ "path": "/home/you/clients/acme", "context": "client-acme" }],
  "git_remote_rules": [{ "pattern": "github.com/acme/*", "context": "client-acme" }]
}
```

A repo can also carry its own binding at `<repo>/.git/info/aisw.json`, which takes precedence over these user-level rules. See [Workspace guardrails](/aisw/workspace/).

## Directory layout

```text
~/.aisw/
├── config.json                        # profile registry and settings (0600)
├── config.json.lock                   # advisory config write lock
├── switch.lock                        # advisory live-switch operation lock
├── workspaces.json                    # workspace binding rules (0600)
├── profiles/
│   ├── claude/
│   │   ├── work/
│   │   │   ├── .credentials.json      # Claude credential file (0600)
│   │   │   └── oauth-account.json     # OAuth account metadata (if OAuth)
│   │   └── personal/
│   ├── codex/
│   │   └── work/
│   │       ├── auth.json              # Codex auth file (0600)
│   │       └── config.toml
│   ├── gemini/
│   │   └── personal/
│   │       ├── .env                   # API-key profiles (GEMINI_API_KEY=...)
│   │       └── oauth_creds.json       # OAuth profiles (0600)
│   └── antigravity/
│       └── work/
│           ├── keyring.json           # captured live keyring metadata
│           └── keyring-secret.json    # secret, when file-backed (0600)
└── backups/
    └── 2026-03-25T11-45-02.123Z-0000/ # backup id
        └── claude/
            └── work/                  # snapshot of the profile directory
```

Profiles stored with `--credential-backend system-keyring` keep their secret in the OS keyring instead of a file in the profile directory.

## Version compatibility

If `config.json` has a schema version higher than your installed `aisw` binary supports, all commands fail with a message asking you to upgrade. Downgrade compatibility (using a newer config with an older binary) is not guaranteed.

This is intentional for contexts: version 2 prevents an older binary from silently rewriting `config.json` and dropping saved context definitions.
