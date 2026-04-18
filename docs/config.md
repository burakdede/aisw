# Configuration

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
