use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::live_apply::LiveFileChange;
use crate::profile::ProfileStore;
use crate::types::Tool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegularFile {
    pub file_name: OsString,
    pub path: PathBuf,
}

pub fn cleanup_profile(profile_store: &ProfileStore, tool: Tool, name: &str) {
    let _ = profile_store.delete(tool, name);
}

pub fn cleanup_profile_on_error<T>(
    result: Result<T>,
    profile_store: &ProfileStore,
    tool: Tool,
    name: &str,
) -> Result<T> {
    result.inspect_err(|_| cleanup_profile(profile_store, tool, name))
}

pub fn apply_profile_file(
    profile_store: &ProfileStore,
    tool: Tool,
    name: &str,
    stored_filename: &str,
    dest: PathBuf,
) -> Result<()> {
    let bytes = profile_store.read_file(tool, name, stored_filename)?;
    crate::live_apply::apply_transaction(vec![LiveFileChange::write(dest, bytes)])
}

pub fn stored_profile_file_matches_live(
    profile_store: &ProfileStore,
    tool: Tool,
    name: &str,
    stored_filename: &str,
    dest: &Path,
) -> Result<bool> {
    if !dest.exists() {
        return Ok(false);
    }
    let live = std::fs::read(dest).with_context(|| format!("could not read {}", dest.display()))?;
    let stored = profile_store.read_file(tool, name, stored_filename)?;
    Ok(live == stored)
}

pub fn list_regular_files(dir: &Path) -> Result<Vec<RegularFile>> {
    let mut files = Vec::new();
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("could not read {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_symlink() || !path.is_file() {
            continue;
        }
        files.push(RegularFile {
            file_name: entry.file_name(),
            path,
        });
    }
    Ok(files)
}

#[cfg(unix)]
pub fn set_permissions_600(path: &Path) -> Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not set permissions on {}", path.display()))
}

#[cfg(not(unix))]
pub fn set_permissions_600(_path: &Path) -> Result<()> {
    Ok(())
}

/// Wrap a shell word in single quotes, escaping any embedded single quotes.
///
/// Shared by the `emit_shell_env` implementations for each tool so the quoting
/// logic stays in one place and the tool modules stay focused on auth.
pub(crate) fn shell_single_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn cleanup_profile_on_error_deletes_profile() {
        let dir = tempdir().unwrap();
        let profile_store = ProfileStore::new(dir.path());
        profile_store.create(Tool::Claude, "work").unwrap();

        let result: Result<()> = Err(anyhow::anyhow!("boom"));
        cleanup_profile_on_error(result, &profile_store, Tool::Claude, "work").unwrap_err();

        assert!(!profile_store.exists(Tool::Claude, "work"));
    }

    #[test]
    fn list_regular_files_skips_directories_and_symlinks() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("real.txt");
        let nested = dir.path().join("nested");
        let link = dir.path().join("link.txt");

        std::fs::write(&file, "x").unwrap();
        std::fs::create_dir(&nested).unwrap();
        symlink(&file, &link).unwrap();

        let files = list_regular_files(dir.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name, OsString::from("real.txt"));
        assert_eq!(files[0].path, file);
    }

    #[test]
    fn apply_and_match_profile_file_round_trip() {
        let dir = tempdir().unwrap();
        let profile_store = ProfileStore::new(dir.path());
        profile_store.create(Tool::Codex, "work").unwrap();
        profile_store
            .write_file(Tool::Codex, "work", "auth.json", br#"{"token":"tok"}"#)
            .unwrap();

        let live = dir.path().join("live-auth.json");
        apply_profile_file(
            &profile_store,
            Tool::Codex,
            "work",
            "auth.json",
            live.clone(),
        )
        .unwrap();

        assert!(stored_profile_file_matches_live(
            &profile_store,
            Tool::Codex,
            "work",
            "auth.json",
            &live
        )
        .unwrap());
    }
}
