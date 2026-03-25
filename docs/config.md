# Configuration

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

Profile directories act as drop-in replacements for each tool's home directory (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). aisw treats the files inside them as opaque blobs — it copies them in and out but never parses their contents.

## Version mismatch

If `config.json` has a `version` higher than the installed aisw supports, aisw will exit with an error and a link to upgrade. It will never silently corrupt a config it does not understand.

Downgrading aisw after upgrading is not supported. Keep backups if you need to roll back.
