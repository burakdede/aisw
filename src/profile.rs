use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::types::Tool;

const NAME_MAX_LEN: usize = 32;

pub struct ProfileStore {
    home: PathBuf,
}

impl ProfileStore {
    pub fn new(home: &Path) -> Self {
        Self {
            home: home.to_owned(),
        }
    }

    pub fn profile_dir(&self, tool: Tool, name: &str) -> PathBuf {
        self.home.join("profiles").join(tool.dir_name()).join(name)
    }

    pub fn exists(&self, tool: Tool, name: &str) -> bool {
        self.profile_dir(tool, name).is_dir()
    }

    pub fn create(&self, tool: Tool, name: &str) -> Result<PathBuf> {
        validate_profile_name(name)?;
        let dir = self.profile_dir(tool, name);
        if dir.exists() {
            bail!(
                "profile '{}' already exists for {}.\n  \
                 Run 'aisw list {}' to see existing profiles, or choose a different name.",
                name,
                tool,
                tool
            );
        }
        fs::create_dir_all(&dir)
            .with_context(|| format!("could not create profile directory {}", dir.display()))?;
        Ok(dir)
    }

    pub fn delete(&self, tool: Tool, name: &str) -> Result<()> {
        let dir = self.profile_dir(tool, name);
        if !dir.is_dir() {
            bail!(
                "profile '{}' not found for {}.\n  \
                 Run 'aisw list {}' to see available profiles.",
                name,
                tool,
                tool
            );
        }
        fs::remove_dir_all(&dir)
            .with_context(|| format!("could not delete profile directory {}", dir.display()))
    }

    pub fn list_profiles(&self, tool: Tool) -> Result<Vec<String>> {
        let base = self.home.join("profiles").join(tool.dir_name());
        if !base.exists() {
            return Ok(vec![]);
        }
        let mut names = vec![];
        for entry in fs::read_dir(&base)
            .with_context(|| format!("could not read directory {}", base.display()))?
        {
            let entry = entry.with_context(|| format!("error reading {}", base.display()))?;
            let path = entry.path();
            // Skip symlinks — we never follow them.
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                if let Some(n) = path.file_name().and_then(|n| n.to_str()) {
                    names.push(n.to_owned());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn write_file(
        &self,
        tool: Tool,
        name: &str,
        filename: &str,
        contents: &[u8],
    ) -> Result<()> {
        let dir = self.profile_dir(tool, name);
        let dest = dir.join(filename);
        reject_symlink(&dest)?;
        let tmp = dest.with_extension("tmp");
        fs::write(&tmp, contents).with_context(|| format!("could not write {}", tmp.display()))?;
        set_permissions_600(&tmp)?;
        fs::rename(&tmp, &dest)
            .with_context(|| format!("could not move file into place at {}", dest.display()))
    }

    pub fn copy_file_into(
        &self,
        tool: Tool,
        name: &str,
        src: &Path,
        dest_filename: &str,
    ) -> Result<()> {
        reject_symlink(src)?;
        let dir = self.profile_dir(tool, name);
        let dest = dir.join(dest_filename);
        reject_symlink(&dest)?;
        fs::copy(src, &dest)
            .with_context(|| format!("could not copy {} to {}", src.display(), dest.display()))?;
        set_permissions_600(&dest)
    }

    pub fn read_file(&self, tool: Tool, name: &str, filename: &str) -> Result<Vec<u8>> {
        let path = self.profile_dir(tool, name).join(filename);
        reject_symlink(&path)?;
        fs::read(&path).with_context(|| format!("could not read {}", path.display()))
    }

    pub fn check_permissions(&self, path: &Path) -> Result<()> {
        check_permissions_600(path)
    }
}

pub fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("profile name must not be empty");
    }
    if name.len() > NAME_MAX_LEN {
        bail!(
            "profile name '{}' exceeds maximum length of {} characters",
            name,
            NAME_MAX_LEN
        );
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "profile name '{}' contains invalid characters (allowed: a-z, A-Z, 0-9, -, _)",
            name
        );
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<()> {
    if path.exists() && path.is_symlink() {
        bail!("refusing to operate on symlink: {}", path.display());
    }
    Ok(())
}

#[cfg(unix)]
fn set_permissions_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_permissions_600(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn check_permissions_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)
        .with_context(|| format!("could not stat {}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o177 != 0 {
        bail!(
            "permissions on {} are too broad (got {:o}, expected 0600)",
            path.display(),
            mode & 0o777
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_permissions_600(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store(dir: &Path) -> ProfileStore {
        ProfileStore::new(dir)
    }

    #[test]
    fn validate_name_ok() {
        for name in &[
            "work",
            "my-profile",
            "work_2",
            "A1",
            "a".repeat(32).as_str(),
        ] {
            assert!(
                validate_profile_name(name).is_ok(),
                "expected ok for '{}'",
                name
            );
        }
    }

    #[test]
    fn validate_name_empty() {
        assert!(validate_profile_name("")
            .unwrap_err()
            .to_string()
            .contains("empty"));
    }

    #[test]
    fn validate_name_too_long() {
        let long = "a".repeat(33);
        let err = validate_profile_name(&long).unwrap_err();
        assert!(err.to_string().contains("exceeds maximum length"));
    }

    #[test]
    fn validate_name_invalid_chars() {
        for name in &["my profile", "work!", "foo/bar", "dot.name"] {
            let err = validate_profile_name(name).unwrap_err();
            assert!(
                err.to_string().contains("invalid characters"),
                "name: {}",
                name
            );
        }
    }

    #[test]
    fn create_and_exists() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        assert!(!s.exists(Tool::Claude, "work"));
        s.create(Tool::Claude, "work").unwrap();
        assert!(s.exists(Tool::Claude, "work"));
    }

    #[test]
    fn create_duplicate_errors() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        s.create(Tool::Claude, "work").unwrap();
        let err = s.create(Tool::Claude, "work").unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert!(err.to_string().contains("aisw list"));
    }

    #[test]
    fn delete_nonexistent_error_mentions_list() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        let err = s.delete(Tool::Claude, "ghost").unwrap_err();
        assert!(err.to_string().contains("not found"));
        assert!(err.to_string().contains("aisw list"));
    }

    #[test]
    fn create_invalid_name_errors() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        assert!(s.create(Tool::Claude, "bad name!").is_err());
    }

    #[test]
    fn delete_profile() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        s.create(Tool::Codex, "personal").unwrap();
        assert!(s.exists(Tool::Codex, "personal"));
        s.delete(Tool::Codex, "personal").unwrap();
        assert!(!s.exists(Tool::Codex, "personal"));
    }

    #[test]
    fn delete_nonexistent_errors() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        let err = s.delete(Tool::Gemini, "ghost").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn list_profiles_empty() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        let profiles = s.list_profiles(Tool::Claude).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn list_profiles_sorted() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        s.create(Tool::Claude, "zebra").unwrap();
        s.create(Tool::Claude, "alpha").unwrap();
        s.create(Tool::Claude, "middle").unwrap();

        let profiles = s.list_profiles(Tool::Claude).unwrap();
        assert_eq!(profiles, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn write_and_read_file() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        s.create(Tool::Claude, "work").unwrap();
        s.write_file(
            Tool::Claude,
            "work",
            ".credentials.json",
            b"{\"token\":\"abc\"}",
        )
        .unwrap();

        let contents = s
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        assert_eq!(contents, b"{\"token\":\"abc\"}");
    }

    #[test]
    #[cfg(unix)]
    fn write_file_sets_600_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let s = store(dir.path());

        s.create(Tool::Claude, "work").unwrap();
        s.write_file(Tool::Claude, "work", "secret.json", b"data")
            .unwrap();

        let path = s.profile_dir(Tool::Claude, "work").join("secret.json");
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn copy_file_into_profile() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Codex, "work").unwrap();

        let src_dir = tempdir().unwrap();
        let src = src_dir.path().join("auth.json");
        fs::write(&src, b"auth-data").unwrap();

        s.copy_file_into(Tool::Codex, "work", &src, "auth.json")
            .unwrap();
        let contents = s.read_file(Tool::Codex, "work", "auth.json").unwrap();
        assert_eq!(contents, b"auth-data");
    }

    #[test]
    #[cfg(unix)]
    fn check_permissions_detects_broad_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Gemini, "default").unwrap();
        s.write_file(Tool::Gemini, "default", "secret.env", b"KEY=val")
            .unwrap();

        let path = s.profile_dir(Tool::Gemini, "default").join("secret.env");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let err = s.check_permissions(&path).unwrap_err();
        assert!(err.to_string().contains("too broad"));
    }

    #[test]
    #[cfg(unix)]
    fn check_permissions_ok_for_600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Claude, "sec").unwrap();
        s.write_file(Tool::Claude, "sec", "creds.json", b"{}")
            .unwrap();

        let path = s.profile_dir(Tool::Claude, "sec").join("creds.json");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(s.check_permissions(&path).is_ok());
    }

    #[test]
    fn list_profiles_ignores_other_tools() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        s.create(Tool::Claude, "work").unwrap();
        s.create(Tool::Codex, "personal").unwrap();

        let claude_profiles = s.list_profiles(Tool::Claude).unwrap();
        assert_eq!(claude_profiles, vec!["work"]);

        let codex_profiles = s.list_profiles(Tool::Codex).unwrap();
        assert_eq!(codex_profiles, vec!["personal"]);
    }
}
