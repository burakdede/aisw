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

pub fn list_regular_files_recursive(dir: &Path) -> Result<Vec<RegularFile>> {
    let mut files = Vec::new();
    list_regular_files_recursive_inner(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn list_regular_files_recursive_inner(
    root: &Path,
    current: &Path,
    files: &mut Vec<RegularFile>,
) -> Result<()> {
    for entry in std::fs::read_dir(current)
        .with_context(|| format!("could not read {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            list_regular_files_recursive_inner(root, &path, files)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("could not relativize {}", path.display()))?;
        files.push(RegularFile {
            file_name: relative.as_os_str().to_owned(),
            path,
        });
    }
    Ok(())
}

pub fn json_equal(a: &[u8], b: &[u8]) -> Result<bool> {
    let va = serde_json::from_slice::<serde_json::Value>(a)
        .context("could not parse JSON for comparison")?;
    let vb = serde_json::from_slice::<serde_json::Value>(b)
        .context("could not parse JSON for comparison")?;
    Ok(va == vb)
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
/// Produces POSIX-compatible quoting (`'value'`, with embedded `'` replaced by
/// `'"'"'`).  Fish also accepts this quoting style, so a single function covers
/// both `emit_export` variants.
pub(crate) fn shell_single_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

/// Emit a shell assignment for `key=value`, choosing the correct syntax for the
/// active shell.
///
/// Fish cannot `eval` POSIX `export KEY=value` lines; it needs `set -gx KEY value`.
/// We detect Fish by checking for `FISH_VERSION`, which Fish always exports into
/// child-process environments.
pub(crate) fn emit_export(key: &str, value: &str) {
    match std::env::var("AISW_SHELL").ok().as_deref() {
        Some("pwsh") => println!("$env:{} = {}", key, powershell_single_quote(value)),
        _ if std::env::var_os("FISH_VERSION").is_some() => {
            println!("set -gx {} {}", key, shell_single_quote(value));
        }
        _ => println!("export {}={}", key, shell_single_quote(value)),
    }
}

/// Emit a shell unset statement for `key`, choosing the correct syntax for the
/// active shell.
pub(crate) fn emit_unset(key: &str) {
    match std::env::var("AISW_SHELL").ok().as_deref() {
        Some("pwsh") => println!("Remove-Item Env:{} -ErrorAction SilentlyContinue", key),
        _ if std::env::var_os("FISH_VERSION").is_some() => println!("set -e {}", key),
        _ => println!("unset {}", key),
    }
}

fn powershell_single_quote(value: &str) -> String {
    let escaped = value.replace('\'', "''");
    format!("'{}'", escaped)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn json_equal_matches_semantically_equivalent_json() {
        assert!(json_equal(br#"{"a":1,"b":2}"#, br#"{"b":2,"a":1}"#).unwrap());
    }

    #[test]
    fn json_equal_differs_on_different_values() {
        assert!(!json_equal(br#"{"a":1}"#, br#"{"a":2}"#).unwrap());
    }

    #[test]
    fn json_equal_propagates_error_on_invalid_first() {
        json_equal(b"not json", br#"{"a":1}"#).unwrap_err();
    }

    #[test]
    fn json_equal_propagates_error_on_invalid_second() {
        json_equal(br#"{"a":1}"#, b"not json").unwrap_err();
    }

    #[test]
    fn cleanup_profile_on_error_deletes_profile() {
        let dir = tempdir().unwrap();
        let profile_store = ProfileStore::new(dir.path());
        profile_store.create(Tool::Claude, "work").unwrap();

        let result: Result<()> = Err(anyhow::anyhow!("boom"));
        cleanup_profile_on_error(result, &profile_store, Tool::Claude, "work").unwrap_err();

        assert!(!profile_store.exists(Tool::Claude, "work"));
    }

    #[cfg(unix)]
    #[test]
    fn list_regular_files_skips_directories_and_symlinks() {
        use std::os::unix::fs::symlink;

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

    #[cfg(unix)]
    #[test]
    fn list_regular_files_recursive_returns_relative_paths() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let root_file = dir.path().join("root.txt");
        let nested_file = nested.join("child.txt");

        std::fs::write(&root_file, "root").unwrap();
        std::fs::write(&nested_file, "child").unwrap();

        let files = list_regular_files_recursive(dir.path()).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file_name, OsString::from("nested/child.txt"));
        assert_eq!(files[1].file_name, OsString::from("root.txt"));
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

    // ---- shell quoting tests ----
    // emit_export / emit_unset are thin wrappers around shell_single_quote plus a
    // FISH_VERSION env check — the quoting correctness is fully covered here.

    #[test]
    fn shell_single_quote_wraps_plain_value() {
        assert_eq!(shell_single_quote("hello"), "'hello'");
    }

    #[test]
    fn shell_single_quote_escapes_embedded_single_quote() {
        assert_eq!(shell_single_quote("it's"), "'it'\"'\"'s'");
    }

    #[test]
    fn shell_single_quote_escapes_path_with_spaces() {
        assert_eq!(
            shell_single_quote("/home/user/my profiles/codex"),
            "'/home/user/my profiles/codex'"
        );
    }

    #[test]
    fn powershell_single_quote_escapes_embedded_single_quote() {
        assert_eq!(powershell_single_quote("it's"), "'it''s'");
    }
}
