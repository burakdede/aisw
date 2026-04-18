---
title: Configuration
description: Config file structure and active-profile state.
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
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Configuration","headline":"Configuration","description":"Config file structure and active-profile state.","url":"https://burakdede.github.io/aisw/configuration/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, configuration, reference","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Configuration","item":"https://burakdede.github.io/aisw/configuration/"}]}]}
---

## Location

Default:

```text
~/.aisw/config.json
```

Override with `AISW_HOME`:

```sh
AISW_HOME=/tmp/aisw-test aisw list
```

## Permissions

`config.json` is written as `0600` (owner read/write only). `aisw status` warns on broader permissions.

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

| Field | Type | Meaning |
|---|---|---|
| `version` | integer | Schema version |
| `active.<tool>` | string or null | Active profile name per tool |
| `profiles.<tool>.<name>` | object | Metadata for stored profile |
| `profiles.<tool>.<name>.added_at` | ISO timestamp | Profile creation time |
| `profiles.<tool>.<name>.auth_method` | `oauth` or `api_key` | Auth mode used when added |
| `profiles.<tool>.<name>.label` | string or null | Optional label |
| `settings.backup_on_switch` | boolean | Create backup before switch |
| `settings.max_backups` | integer | Max backups to keep |

Credentials are not stored in `config.json`; they are stored under `~/.aisw/profiles/...`.

## Directory layout

```text
~/.aisw/
├── config.json
├── profiles/
│   ├── claude/<profile>/
│   ├── codex/<profile>/
│   └── gemini/<profile>/
└── backups/
```

## Version compatibility

If `config.json` has a higher schema version than your installed `aisw`, commands fail with an upgrade message. Downgrade compatibility is not guaranteed.
