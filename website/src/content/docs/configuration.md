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
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, configuration, reference
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Configuration","headline":"Configuration","description":"aisw configuration file location, schema, field reference, directory layout, and AISW_HOME override.","url":"https://burakdede.github.io/aisw/configuration/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, configuration, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Configuration","item":"https://burakdede.github.io/aisw/configuration/"}]}]}
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
  "version": 1,
  "active": {
    "claude": "work",
    "codex": null,
    "gemini": null
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
    "gemini": {}
  },
  "settings": {
    "backup_on_switch": true,
    "max_backups": 10
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
| `settings.backup_on_switch` | boolean | Create a backup before activating a profile. Default: true. |
| `settings.max_backups` | integer | Maximum number of backups to retain. Older ones are pruned when the limit is exceeded. Default: 10. |

Credentials are stored under `~/.aisw/profiles/`, not in `config.json`.

## Directory layout

```text
~/.aisw/
├── config.json                        # profile registry and settings
├── profiles/
│   ├── claude/
│   │   ├── work/
│   │   │   ├── .credentials.json      # Claude credential file (0600)
│   │   │   └── oauth_account.json     # OAuth account metadata (if OAuth)
│   │   └── personal/
│   ├── codex/
│   │   └── work/
│   │       ├── auth.json              # Codex auth file (0600)
│   │       └── config.toml
│   └── gemini/
│       └── personal/
│           ├── oauth_creds.json       # Gemini OAuth cache (0600)
│           └── settings.json
└── backups/
    └── 20260325T114502Z-claude-work/  # timestamped backup snapshot
```

## Version compatibility

If `config.json` has a schema version higher than your installed `aisw` binary supports, all commands fail with a message asking you to upgrade. Downgrade compatibility (using a newer config with an older binary) is not guaranteed.
