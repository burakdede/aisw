//! Path resolution for Claude Code's live credential and metadata files.
//!
//! Claude stores credentials in one of two locations depending on how it was
//! installed. The secondary XDG path (`~/.config/claude/`) is preferred only
//! when it exists and the primary (`~/.claude/`) does not.

use std::path::{Path, PathBuf};

/// Returns the path to the live `.credentials.json` file, preferring the XDG
/// secondary location only when it exists and the primary does not.
pub(super) fn live_credentials_path(user_home: &Path) -> PathBuf {
    let primary = user_home.join(".claude").join(super::CREDENTIALS_FILE);
    let secondary = user_home
        .join(".config")
        .join("claude")
        .join(super::CREDENTIALS_FILE);

    if secondary.exists() && !primary.exists() {
        secondary
    } else {
        primary
    }
}

/// Returns both possible live credentials paths in priority order.
pub(super) fn live_credentials_paths(user_home: &Path) -> [PathBuf; 2] {
    [
        user_home.join(".claude").join(super::CREDENTIALS_FILE),
        user_home
            .join(".config")
            .join("claude")
            .join(super::CREDENTIALS_FILE),
    ]
}

/// Returns the path to `~/.claude.json`, where Claude stores OAuth account
/// metadata (`oauthAccount` field).
pub(super) fn live_account_metadata_path(user_home: &Path) -> PathBuf {
    user_home.join(".claude.json")
}

/// Returns the Claude local state directory if it exists. Returns `None` when
/// neither `~/.claude/` nor `~/.config/claude/` is present.
pub fn live_local_state_dir(user_home: &Path) -> Option<PathBuf> {
    let primary = user_home.join(".claude");
    if primary.exists() {
        return Some(primary);
    }

    let secondary = user_home.join(".config").join("claude");
    if secondary.exists() {
        Some(secondary)
    } else {
        None
    }
}
