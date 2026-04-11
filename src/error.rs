//! Typed, user-facing error taxonomy for aisw.
//!
//! Every variant maps to a stable snake_case `code()` string and a documented
//! process exit code. Internal, non-user-facing errors continue to travel as
//! plain `anyhow::Error`; `AiswError` lives at the command-dispatch boundary.
//!
//! ## Adding a new variant
//!
//! 1. Add the variant to the `AiswError` enum below.
//! 2. Add a `code()` arm (snake_case, no spaces).
//! 3. Add an `exit_code()` arm (prefer reusing an existing code unless the new
//!    error category is meaningfully distinct for scripting).
//! 4. Add a `Display` arm with a human-readable message.
//! 5. Add unit tests for `Display` and `code()`.
//! 6. Convert relevant `anyhow::bail!` / `anyhow::anyhow!` call sites that
//!    belong to this category.

use std::fmt;
use std::path::PathBuf;

use crate::types::Tool;

/// Exit code for all errors not mapped to a specific `AiswError` variant.
pub const EXIT_GENERAL_ERROR: i32 = 1;

#[derive(Debug)]
pub enum AiswError {
    /// A named profile was not found in the config for the given tool.
    ProfileNotFound { tool: Tool, name: String },
    /// A profile with the given name already exists for the tool.
    ProfileAlreadyExists { tool: Tool, name: String },
    /// The required tool binary is not installed or not on PATH.
    ToolNotInstalled { tool: Tool },
    /// The OS keyring is unavailable (daemon not running, unsupported platform,
    /// etc.).
    KeyringUnavailable { reason: String },
    /// The config file could not be exclusively locked within the timeout.
    ConfigLocked,
    /// The config file exists but could not be parsed as valid JSON.
    ConfigCorrupt { reason: String },
    /// Authentication with the tool failed (OAuth capture timeout, bad key,
    /// etc.).
    AuthFailed { tool: Tool, reason: String },
    /// A file operation was denied due to insufficient permissions.
    PermissionDenied { path: PathBuf },
    /// A backup with the given ID does not exist.
    BackupNotFound { id: String },
}

impl AiswError {
    /// A stable, snake_case identifier for machine-readable output (`--json`).
    pub fn code(&self) -> &'static str {
        match self {
            AiswError::ProfileNotFound { .. } => "profile_not_found",
            AiswError::ProfileAlreadyExists { .. } => "profile_already_exists",
            AiswError::ToolNotInstalled { .. } => "tool_not_installed",
            AiswError::KeyringUnavailable { .. } => "keyring_unavailable",
            AiswError::ConfigLocked => "config_locked",
            AiswError::ConfigCorrupt { .. } => "config_corrupt",
            AiswError::AuthFailed { .. } => "auth_failed",
            AiswError::PermissionDenied { .. } => "permission_denied",
            AiswError::BackupNotFound { .. } => "backup_not_found",
        }
    }

    /// Process exit code. Use `EXIT_GENERAL_ERROR` (1) for errors that don't
    /// need to be distinguishable by scripts, and a unique value only when the
    /// distinction is actionable by a caller.
    pub fn exit_code(&self) -> i32 {
        match self {
            // Exit 2: profile not found — scripts can distinguish "wrong name"
            // from "something broke internally".
            AiswError::ProfileNotFound { .. } => 2,
            // All other structured errors use the standard error exit code.
            _ => EXIT_GENERAL_ERROR,
        }
    }
}

impl fmt::Display for AiswError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiswError::ProfileNotFound { tool, name } => write!(
                f,
                "profile '{name}' not found for {tool}.\n  \
                 Run 'aisw list {tool}' to see available profiles."
            ),
            AiswError::ProfileAlreadyExists { tool, name } => write!(
                f,
                "profile '{name}' already exists for {tool}.\n  \
                 Use 'aisw list {tool}' to see existing profiles."
            ),
            AiswError::ToolNotInstalled { tool } => write!(
                f,
                "{} is not installed or not on PATH.\n  \
                 Install it and retry, or use 'aisw doctor' to diagnose.",
                tool.display_name()
            ),
            AiswError::KeyringUnavailable { reason } => write!(
                f,
                "OS keyring is unavailable: {reason}.\n  \
                 Use --credential-backend=file to store credentials in the \
                 profile directory instead."
            ),
            AiswError::ConfigLocked => write!(
                f,
                "aisw config is locked by another process.\n  \
                 Wait a moment and retry. If this persists, check for a \
                 stale lock on ~/.aisw/config.json."
            ),
            AiswError::ConfigCorrupt { reason } => write!(
                f,
                "aisw config is corrupt and could not be parsed: {reason}.\n  \
                 Back up and remove ~/.aisw/config.json, then run 'aisw init'."
            ),
            AiswError::AuthFailed { tool, reason } => write!(
                f,
                "authentication failed for {}: {reason}.",
                tool.display_name()
            ),
            AiswError::PermissionDenied { path } => write!(
                f,
                "permission denied: {}.\n  \
                 Check file ownership and permissions (expected 0600).",
                path.display()
            ),
            AiswError::BackupNotFound { id } => write!(
                f,
                "backup '{id}' not found.\n  \
                 Run 'aisw backup list' to see available backups."
            ),
        }
    }
}

impl std::error::Error for AiswError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Tool;

    // ---- code() tests ----

    #[test]
    fn profile_not_found_code() {
        let e = AiswError::ProfileNotFound {
            tool: Tool::Claude,
            name: "work".into(),
        };
        assert_eq!(e.code(), "profile_not_found");
    }

    #[test]
    fn profile_already_exists_code() {
        let e = AiswError::ProfileAlreadyExists {
            tool: Tool::Codex,
            name: "ci".into(),
        };
        assert_eq!(e.code(), "profile_already_exists");
    }

    #[test]
    fn tool_not_installed_code() {
        assert_eq!(
            AiswError::ToolNotInstalled { tool: Tool::Gemini }.code(),
            "tool_not_installed"
        );
    }

    #[test]
    fn keyring_unavailable_code() {
        assert_eq!(
            AiswError::KeyringUnavailable {
                reason: "daemon not running".into()
            }
            .code(),
            "keyring_unavailable"
        );
    }

    #[test]
    fn config_locked_code() {
        assert_eq!(AiswError::ConfigLocked.code(), "config_locked");
    }

    #[test]
    fn config_corrupt_code() {
        assert_eq!(
            AiswError::ConfigCorrupt {
                reason: "unexpected eof".into()
            }
            .code(),
            "config_corrupt"
        );
    }

    #[test]
    fn auth_failed_code() {
        assert_eq!(
            AiswError::AuthFailed {
                tool: Tool::Claude,
                reason: "timeout".into()
            }
            .code(),
            "auth_failed"
        );
    }

    #[test]
    fn permission_denied_code() {
        assert_eq!(
            AiswError::PermissionDenied {
                path: PathBuf::from("/some/path")
            }
            .code(),
            "permission_denied"
        );
    }

    #[test]
    fn backup_not_found_code() {
        assert_eq!(
            AiswError::BackupNotFound {
                id: "2026-01-01T00-00-00.000Z-0".into()
            }
            .code(),
            "backup_not_found"
        );
    }

    // ---- exit_code() tests ----

    #[test]
    fn profile_not_found_exits_2() {
        let e = AiswError::ProfileNotFound {
            tool: Tool::Claude,
            name: "x".into(),
        };
        assert_eq!(e.exit_code(), 2);
    }

    #[test]
    fn all_other_variants_exit_1() {
        let cases: &[AiswError] = &[
            AiswError::ProfileAlreadyExists {
                tool: Tool::Claude,
                name: "x".into(),
            },
            AiswError::ToolNotInstalled { tool: Tool::Codex },
            AiswError::KeyringUnavailable {
                reason: "n/a".into(),
            },
            AiswError::ConfigLocked,
            AiswError::ConfigCorrupt {
                reason: "n/a".into(),
            },
            AiswError::AuthFailed {
                tool: Tool::Gemini,
                reason: "n/a".into(),
            },
            AiswError::PermissionDenied {
                path: PathBuf::from("/x"),
            },
            AiswError::BackupNotFound { id: "x".into() },
        ];
        for case in cases {
            assert_eq!(
                case.exit_code(),
                EXIT_GENERAL_ERROR,
                "{} should exit 1",
                case.code()
            );
        }
    }

    // ---- Display tests ----

    #[test]
    fn profile_not_found_display_contains_name_and_tool() {
        let e = AiswError::ProfileNotFound {
            tool: Tool::Claude,
            name: "work".into(),
        };
        let s = e.to_string();
        assert!(s.contains("work"), "missing name: {s}");
        assert!(s.contains("claude"), "missing tool: {s}");
        assert!(s.contains("not found"), "missing 'not found': {s}");
    }

    #[test]
    fn profile_already_exists_display_contains_name_and_tool() {
        let e = AiswError::ProfileAlreadyExists {
            tool: Tool::Codex,
            name: "ci".into(),
        };
        let s = e.to_string();
        assert!(s.contains("ci"), "missing name: {s}");
        assert!(s.contains("codex"), "missing tool: {s}");
        assert!(
            s.contains("already exists"),
            "missing 'already exists': {s}"
        );
    }

    #[test]
    fn tool_not_installed_display_mentions_tool_name() {
        let e = AiswError::ToolNotInstalled { tool: Tool::Gemini };
        assert!(e.to_string().contains("Gemini CLI"));
    }

    #[test]
    fn backup_not_found_display_contains_id() {
        let id = "2026-01-01T00-00-00.000Z-0";
        let e = AiswError::BackupNotFound { id: id.into() };
        assert!(e.to_string().contains(id));
    }

    // ---- std::error::Error integration ----

    #[test]
    fn aisw_error_wraps_in_anyhow() {
        let e = AiswError::ProfileNotFound {
            tool: Tool::Claude,
            name: "missing".into(),
        };
        let wrapped = anyhow::Error::from(e);
        // Downcast must succeed.
        assert!(wrapped.downcast_ref::<AiswError>().is_some());
    }

    #[test]
    fn downcast_preserves_code() {
        let wrapped = anyhow::Error::from(AiswError::ProfileNotFound {
            tool: Tool::Codex,
            name: "x".into(),
        });
        let ae = wrapped.downcast_ref::<AiswError>().unwrap();
        assert_eq!(ae.code(), "profile_not_found");
    }
}
