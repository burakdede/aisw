---
title: Configuration
description: Reference the aisw config file, active profile state, and stored settings.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/config.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, Configuration, reference, aisw config.json, aisw configuration file
  - tag: meta
    attrs:
      property: article:section
      content: reference
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Configuration","headline":"Configuration","description":"Reference the aisw config file, active profile state, and stored settings.","url":"https://burakdede.github.io/aisw/configuration/","inLanguage":"en","keywords":"aisw, AI Switcher, AI CLI account switcher, AI account manager, AI CLI account manager, coding agent account manager, coding agent account switcher, Claude Code, Codex CLI, Gemini CLI, multi-account CLI, developer tooling, aisw config.json, aisw configuration file","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","alternateName":"AI Switcher","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.1.1","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Configuration","item":"https://burakdede.github.io/aisw/configuration/"}]}]}
---

## Location

aisw stores its configuration at `~/.aisw/config.json`. To use a different directory, set the `AISW_HOME` environment variable:

```
AISW_HOME=/path/to/custom/dir aisw list
```

This is useful for testing or for running multiple isolated aisw environments.

## File permissions

`config.json` is written with `0600` permissions (owner read/write only). aisw will warn if it finds the file with broader permissions.

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
        "label": "Work Max subscription"
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

### Fields

| Field | Type | Description |
|---|---|---|
| `version` | integer | Schema version. aisw will refuse to load a config with a version higher than it supports. |
| `active.<tool>` | string or null | The currently active profile name for each tool. Null means no profile is active. |
| `profiles.<tool>.<name>` | object | Metadata for a stored profile. Does not contain credentials — those live in the profile directory. |
| `profiles.<tool>.<name>.added_at` | ISO 8601 timestamp | When the profile was added. |
| `profiles.<tool>.<name>.auth_method` | `"oauth"` or `"api_key"` | How the profile authenticates. |
| `profiles.<tool>.<name>.label` | string or null | Optional human-readable description. |
| `settings.backup_on_switch` | boolean | Whether to create a backup before every profile switch. Default: true. |
| `settings.max_backups` | integer | Maximum number of backups to keep. Older ones are pruned automatically. Default: 10. |

## Storage layout

```
~/.aisw/
├── config.json
├── profiles/
│   ├── claude/
│   │   └── work/
│   │       └── .credentials.json
│   ├── codex/
│   │   └── work/
│   │       ├── auth.json
│   │       └── config.toml
│   └── gemini/
│       └── default/
│           └── .env
└── backups/
    └── 2026-03-25T10-00-00Z/
        └── claude/
            └── work/
                └── .credentials.json
```

Profile directories store the per-profile credential state that `aisw` copies into each tool's live config location on `use`. aisw treats most credential files as opaque blobs — it copies them in and out but does not validate token contents.

## Version mismatch

If `config.json` has a `version` higher than the installed aisw supports, aisw will exit with an error and a link to upgrade. It will never silently corrupt a config it does not understand.

Downgrading aisw after upgrading is not supported. Keep backups if you need to roll back.
