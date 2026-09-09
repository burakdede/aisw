//! Path resolution for Claude Code's live credential and metadata files.
//!
//! Claude stores credentials in one of two locations depending on how it was
//! installed. The secondary XDG path (`~/.config/claude/`) is preferred only
//! when it exists and the primary (`~/.claude/`) does not.

use std::path::{Path, PathBuf};

fn configured_dir(user_home: &Path) -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                path
            } else {
                user_home.join(path)
            }
        })
}

/// Returns the path to the live `.credentials.json` file, preferring the XDG
/// secondary location only when it exists and the primary does not. An
/// explicit `CLAUDE_CONFIG_DIR` takes precedence over both defaults.
pub(super) fn live_credentials_path(user_home: &Path) -> PathBuf {
    if let Some(config_dir) = configured_dir(user_home) {
        return config_dir.join(super::CREDENTIALS_FILE);
    }

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
pub(super) fn live_credentials_paths(user_home: &Path) -> Vec<PathBuf> {
    if let Some(config_dir) = configured_dir(user_home) {
        return vec![config_dir.join(super::CREDENTIALS_FILE)];
    }

    vec![
        user_home.join(".claude").join(super::CREDENTIALS_FILE),
        user_home
            .join(".config")
            .join("claude")
            .join(super::CREDENTIALS_FILE),
    ]
}

pub(super) fn live_credentials_root(user_home: &Path) -> PathBuf {
    configured_dir(user_home).unwrap_or_else(|| user_home.to_owned())
}

/// Returns the path to `~/.claude.json`, where Claude stores OAuth account
/// metadata (`oauthAccount` field).
pub(super) fn live_account_metadata_path(user_home: &Path) -> PathBuf {
    user_home.join(".claude.json")
}

/// Returns the Claude local state directory if it exists. An explicit
/// `CLAUDE_CONFIG_DIR` takes precedence over the default locations.
pub fn live_local_state_dir(user_home: &Path) -> Option<PathBuf> {
    if let Some(config_dir) = configured_dir(user_home) {
        return config_dir.exists().then_some(config_dir);
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::test_overrides::EnvVarGuard;
    use tempfile::tempdir;

    #[test]
    fn configured_dir_replaces_default_credential_locations() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let configured = dir.path().join("claude-state");
        std::fs::create_dir_all(user_home.join(".claude")).unwrap();
        std::fs::create_dir_all(&configured).unwrap();
        let _config = EnvVarGuard::set("CLAUDE_CONFIG_DIR", &configured);

        assert_eq!(
            live_credentials_path(&user_home),
            configured.join(super::super::CREDENTIALS_FILE)
        );
        assert_eq!(
            live_credentials_paths(&user_home),
            vec![configured.join(super::super::CREDENTIALS_FILE)]
        );
        assert_eq!(live_credentials_root(&user_home), configured);
        assert_eq!(live_local_state_dir(&user_home), Some(configured));
    }

    #[test]
    fn relative_configured_dir_is_resolved_under_user_home() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let _config = EnvVarGuard::set("CLAUDE_CONFIG_DIR", "custom-claude");

        assert_eq!(
            live_credentials_path(&user_home),
            user_home
                .join("custom-claude")
                .join(super::super::CREDENTIALS_FILE)
        );
    }

    #[test]
    fn default_locations_remain_when_configured_dir_is_unset() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let _config = EnvVarGuard::set("CLAUDE_CONFIG_DIR", "");

        assert_eq!(
            live_credentials_path(&user_home),
            user_home
                .join(".claude")
                .join(super::super::CREDENTIALS_FILE)
        );
        assert_eq!(live_credentials_root(&user_home), user_home);
    }
}
