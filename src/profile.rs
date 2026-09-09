use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

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

    /// Path a profile's files live under.
    ///
    /// This is the display/lookup form and does not validate `name`. Every
    /// operation that reads or writes through the returned path goes via
    /// [`Self::validated_profile_dir`] so a name that escaped validation
    /// elsewhere can never resolve outside the profiles tree.
    pub fn profile_dir(&self, tool: Tool, name: &str) -> PathBuf {
        self.home.join("profiles").join(tool.dir_name()).join(name)
    }

    pub fn validated_profile_dir(&self, tool: Tool, name: &str) -> Result<PathBuf> {
        validate_profile_name(name)?;
        let dir = self.profile_dir(tool, name);
        reject_symlinked_components(&self.home, &dir)?;
        Ok(dir)
    }

    pub fn exists(&self, tool: Tool, name: &str) -> bool {
        self.validated_profile_dir(tool, name)
            .is_ok_and(|dir| dir.is_dir())
    }

    pub fn create(&self, tool: Tool, name: &str) -> Result<PathBuf> {
        validate_profile_name(name)?;
        let dir = self.profile_dir(tool, name);
        reject_symlinked_components(&self.home, &dir)?;
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
        let dir = self.validated_profile_dir(tool, name)?;
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

    pub fn rename(&self, tool: Tool, old_name: &str, new_name: &str) -> Result<()> {
        let old_dir = self.validated_profile_dir(tool, old_name)?;
        let new_dir = self.validated_profile_dir(tool, new_name)?;

        if old_name == new_name {
            bail!("profile '{}' is already named '{}'.", old_name, new_name);
        }
        if !old_dir.is_dir() {
            bail!(
                "profile '{}' not found for {}.\n  \
                 Run 'aisw list {}' to see available profiles.",
                old_name,
                tool,
                tool
            );
        }
        if new_dir.exists() {
            bail!(
                "profile '{}' already exists for {}.\n  \
                 Run 'aisw list {}' to see existing profiles, or choose a different name.",
                new_name,
                tool,
                tool
            );
        }

        fs::rename(&old_dir, &new_dir).with_context(|| {
            format!(
                "could not rename profile directory from {} to {}",
                old_dir.display(),
                new_dir.display()
            )
        })
    }

    pub fn list_profiles(&self, tool: Tool) -> Result<Vec<String>> {
        let base = self.home.join("profiles").join(tool.dir_name());
        reject_symlinked_components(&self.home, &base)?;
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
        let dest = self.profile_file_path(tool, name, filename)?;
        reject_symlink(&dest)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let tmp = staging_path_for(&dest);
        if let Err(error) =
            fs::write(&tmp, contents).with_context(|| format!("could not write {}", tmp.display()))
        {
            return Err(cleanup_staged_file_after_error(&tmp, error));
        }
        if let Err(error) = set_permissions_600(&tmp) {
            return Err(cleanup_staged_file_after_error(&tmp, error));
        }
        if let Err(error) = fs::rename(&tmp, &dest)
            .with_context(|| format!("could not move file into place at {}", dest.display()))
        {
            return Err(cleanup_staged_file_after_error(&tmp, error));
        }
        Ok(())
    }

    pub fn copy_file_into(
        &self,
        tool: Tool,
        name: &str,
        src: &Path,
        dest_filename: &str,
    ) -> Result<()> {
        reject_symlink(src)?;
        let dest = self.profile_file_path(tool, name, dest_filename)?;
        reject_symlink(&dest)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        fs::copy(src, &dest)
            .with_context(|| format!("could not copy {} to {}", src.display(), dest.display()))?;
        set_permissions_600(&dest)
    }

    pub fn read_file(&self, tool: Tool, name: &str, filename: &str) -> Result<Vec<u8>> {
        let path = self.profile_file_path(tool, name, filename)?;
        reject_symlink(&path)?;
        fs::read(&path).with_context(|| format!("could not read {}", path.display()))
    }

    /// Resolve `filename` inside a profile directory, rejecting anything that
    /// would escape it (absolute paths, `..`, drive/root prefixes).
    fn profile_file_path(&self, tool: Tool, name: &str, filename: &str) -> Result<PathBuf> {
        let dir = self.validated_profile_dir(tool, name)?;
        validate_relative_filename(filename)?;
        let path = dir.join(filename);
        reject_symlinked_components(&dir, &path)?;
        Ok(path)
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

/// Reject a path that is itself a symlink.
///
/// Uses `symlink_metadata` rather than `Path::exists`: `exists()` follows
/// links, so a *dangling* symlink reports as absent and would slip through,
/// letting a later write create the link's target instead of the intended file.
fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            bail!("refusing to operate on symlink: {}", path.display())
        }
        _ => Ok(()),
    }
}

fn reject_symlinked_components(root: &Path, path: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("path {} is outside {}", path.display(), root.display()))?;
    let mut current = root.to_owned();
    for component in relative.components() {
        if let Component::Normal(part) = component {
            current.push(part);
            reject_symlink(&current)?;
        }
    }
    Ok(())
}

/// Validate a profile-relative file name such as `auth.json` or
/// `nested/state.json`.
fn validate_relative_filename(filename: &str) -> Result<()> {
    if filename.is_empty() {
        bail!("profile file name must not be empty");
    }

    let path = Path::new(filename);
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => bail!(
                "profile file name '{}' must stay inside the profile directory",
                filename
            ),
        }
    }
    Ok(())
}

/// Sibling temp path used to stage an atomic write.
///
/// Appends a suffix instead of replacing the extension so that sibling files
/// sharing a stem (`auth.json` and `auth.toml`) never stage through the same
/// temp path, and includes the pid so concurrent processes do not collide.
fn staging_path_for(dest: &Path) -> PathBuf {
    let file_name = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "profile".to_owned());
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{}.aisw-tmp-{}", file_name, std::process::id()))
}

fn cleanup_staged_file_after_error(path: &Path, error: anyhow::Error) -> anyhow::Error {
    match fs::remove_file(path) {
        Ok(()) => error,
        Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => error,
        Err(cleanup_error) => anyhow!(
            "{error:#}\nStaged-file cleanup also failed: {}",
            cleanup_error
        ),
    }
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

    /// A name that escaped validation elsewhere must never resolve outside the
    /// profiles tree, even though `profile_dir` itself does not validate.
    #[test]
    fn traversal_profile_names_are_rejected_by_every_file_operation() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        for name in ["../escape", "..", "a/b", "/abs"] {
            assert!(
                !s.exists(Tool::Claude, name),
                "exists() must not accept '{name}'"
            );
            assert!(s.delete(Tool::Claude, name).is_err(), "delete '{name}'");
            assert!(
                s.write_file(Tool::Claude, name, "f.json", b"{}").is_err(),
                "write_file '{name}'"
            );
            assert!(
                s.read_file(Tool::Claude, name, "f.json").is_err(),
                "read_file '{name}'"
            );
            assert!(
                s.rename(Tool::Claude, name, "ok").is_err(),
                "rename from '{name}'"
            );
        }
    }

    /// A stored file name must not be able to climb out of its profile.
    #[test]
    fn traversal_file_names_are_rejected() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Claude, "work").unwrap();

        let outside = dir.path().join("stolen.json");
        for filename in ["../../stolen.json", "/etc/passwd", ""] {
            let err = s
                .write_file(Tool::Claude, "work", filename, b"secret")
                .unwrap_err();
            assert!(
                err.to_string().contains("profile file name"),
                "unexpected error for '{filename}': {err}"
            );
        }
        assert!(!outside.exists(), "write must not escape the profile dir");
    }

    /// `Path::exists` follows symlinks, so a *dangling* link previously slipped
    /// past the symlink guard and the write created the link's target.
    #[test]
    #[cfg(unix)]
    fn write_file_refuses_to_follow_a_dangling_symlink() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Codex, "work").unwrap();

        let target = dir.path().join("outside-target.json");
        let link = s.profile_dir(Tool::Codex, "work").join("auth.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(!target.exists(), "link target starts out dangling");

        let err = s
            .write_file(Tool::Codex, "work", "auth.json", b"secret")
            .unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected a symlink refusal, got: {err}"
        );
        assert!(
            !target.exists(),
            "write must not create the symlink's target"
        );
    }

    #[test]
    #[cfg(unix)]
    fn write_file_refuses_a_symlinked_profile_directory() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        let tool_dir = dir.path().join("profiles").join(Tool::Codex.dir_name());
        std::fs::create_dir_all(&tool_dir).unwrap();

        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, tool_dir.join("work")).unwrap();

        let err = s
            .write_file(Tool::Codex, "work", "auth.json", b"secret")
            .unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected a symlink refusal, got: {err}"
        );
        assert!(
            !outside.join("auth.json").exists(),
            "write must not follow a symlinked profile directory"
        );
    }

    #[test]
    #[cfg(unix)]
    fn write_file_refuses_a_symlinked_nested_directory() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Codex, "work").unwrap();

        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let nested = s.profile_dir(Tool::Codex, "work").join("nested");
        std::os::unix::fs::symlink(&outside, &nested).unwrap();

        let err = s
            .write_file(Tool::Codex, "work", "nested/auth.json", b"secret")
            .unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected a symlink refusal, got: {err}"
        );
        assert!(
            !outside.join("auth.json").exists(),
            "write must not follow a symlinked nested directory"
        );
    }

    #[test]
    fn write_file_cleans_staged_secret_when_rename_fails() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Codex, "work").unwrap();
        let profile_dir = s.profile_dir(Tool::Codex, "work");
        std::fs::create_dir(profile_dir.join("auth.json")).unwrap();

        let err = s
            .write_file(Tool::Codex, "work", "auth.json", b"secret")
            .unwrap_err();

        assert!(err.to_string().contains("could not move file into place"));
        let entries: Vec<_> = std::fs::read_dir(profile_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            entries.iter().all(|name| !name.contains(".aisw-tmp-")),
            "staged files should be removed after rename failure: {entries:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn list_profiles_refuses_a_symlinked_tool_directory() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let tool_dir = dir.path().join("profiles").join(Tool::Codex.dir_name());
        std::fs::create_dir_all(tool_dir.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside, &tool_dir).unwrap();

        let err = s.list_profiles(Tool::Codex).unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected a symlink refusal, got: {err}"
        );
    }

    /// Sibling files sharing a stem must not stage through the same temp path.
    #[test]
    fn files_sharing_a_stem_do_not_collide_while_staging() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Codex, "work").unwrap();

        s.write_file(Tool::Codex, "work", "auth.json", b"json")
            .unwrap();
        s.write_file(Tool::Codex, "work", "auth.toml", b"toml")
            .unwrap();

        assert_eq!(
            s.read_file(Tool::Codex, "work", "auth.json").unwrap(),
            b"json"
        );
        assert_eq!(
            s.read_file(Tool::Codex, "work", "auth.toml").unwrap(),
            b"toml"
        );
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
    fn rename_updates_profile_directory() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        s.create(Tool::Claude, "default").unwrap();
        s.rename(Tool::Claude, "default", "work").unwrap();

        assert!(!s.exists(Tool::Claude, "default"));
        assert!(s.exists(Tool::Claude, "work"));
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
    fn write_file_creates_nested_parent_directories() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());

        s.create(Tool::Gemini, "work").unwrap();
        s.write_file(Tool::Gemini, "work", "nested/deeper/state.json", b"{}")
            .unwrap();

        let contents = s
            .read_file(Tool::Gemini, "work", "nested/deeper/state.json")
            .unwrap();
        assert_eq!(contents, b"{}");
    }

    #[test]
    fn copy_file_into_creates_nested_parent_directories() {
        let dir = tempdir().unwrap();
        let s = store(dir.path());
        s.create(Tool::Codex, "work").unwrap();

        let src_dir = tempdir().unwrap();
        let src = src_dir.path().join("auth.json");
        fs::write(&src, b"auth-data").unwrap();

        s.copy_file_into(Tool::Codex, "work", &src, "nested/auth.json")
            .unwrap();
        let contents = s
            .read_file(Tool::Codex, "work", "nested/auth.json")
            .unwrap();
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
