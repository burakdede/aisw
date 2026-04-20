use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::secure_store;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::Tool;

const BACKUPS_DIR: &str = "backups";
const METADATA_FILE: &str = ".aisw-backup.json";
static BACKUP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct BackupEntry {
    pub backup_id: String,
    pub tool: Tool,
    pub profile: String,
}

pub struct BackupManager {
    home: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupProfileMetadata {
    profile_meta: ProfileMeta,
}

impl BackupManager {
    pub fn new(home: &Path) -> Self {
        Self {
            home: home.to_owned(),
        }
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.home.join(BACKUPS_DIR)
    }

    /// Snapshot all files in `profile_dir` into a uniquely identified backup directory.
    /// Returns the path of the created backup directory.
    pub fn snapshot(
        &self,
        tool: Tool,
        name: &str,
        profile_dir: &Path,
        profile_meta: &ProfileMeta,
    ) -> Result<PathBuf> {
        profile_meta.credential_backend.validate_for_tool(tool)?;
        let backup_id = backup_id_now();
        let dest = self
            .backups_dir()
            .join(&backup_id)
            .join(tool.dir_name())
            .join(name);
        fs::create_dir_all(&dest)
            .with_context(|| format!("could not create backup directory {}", dest.display()))?;

        if profile_meta.credential_backend == CredentialBackend::SystemKeyring {
            secure_store::snapshot_profile_secret(tool, name, &backup_id)?;
        }

        for entry in fs::read_dir(profile_dir)
            .with_context(|| format!("could not read profile dir {}", profile_dir.display()))?
        {
            let entry = entry?;
            let src = entry.path();
            if src.is_symlink() || !src.is_file() {
                continue;
            }
            let filename = entry.file_name();
            let dst = dest.join(&filename);
            fs::copy(&src, &dst).with_context(|| {
                format!("could not copy {} to {}", src.display(), dst.display())
            })?;
            set_permissions_600(&dst)?;
        }

        write_metadata(
            &dest.join(METADATA_FILE),
            &BackupProfileMetadata {
                profile_meta: profile_meta.clone(),
            },
        )?;

        Ok(dest)
    }

    /// List all backup entries, sorted newest-first.
    pub fn list(&self) -> Result<Vec<BackupEntry>> {
        let base = self.backups_dir();
        if !base.exists() {
            return Ok(vec![]);
        }

        let mut entries = vec![];

        for ts_entry in fs::read_dir(&base)
            .with_context(|| format!("could not read backups dir {}", base.display()))?
        {
            let ts_entry = ts_entry?;
            let ts_path = ts_entry.path();
            if !ts_path.is_dir() || ts_path.is_symlink() {
                continue;
            }
            let backup_id = ts_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_owned();

            for tool in Tool::ALL {
                let tool_path = ts_path.join(tool.dir_name());
                if !tool_path.is_dir() {
                    continue;
                }
                for profile_entry in fs::read_dir(&tool_path)? {
                    let profile_entry = profile_entry?;
                    let profile_path = profile_entry.path();
                    if !profile_path.is_dir() || profile_path.is_symlink() {
                        continue;
                    }
                    if let Some(profile) = profile_path.file_name().and_then(|n| n.to_str()) {
                        entries.push(BackupEntry {
                            backup_id: backup_id.clone(),
                            tool,
                            profile: profile.to_owned(),
                        });
                    }
                }
            }
        }

        // Sort newest-first (backup ids are lexicographically sortable).
        entries.sort_by(|a, b| b.backup_id.cmp(&a.backup_id));
        Ok(entries)
    }

    /// Restore files from the backup identified by `backup_id` back into the
    /// corresponding profile directory, enforcing 0600 on all restored files.
    pub fn restore(
        &self,
        backup_id: &str,
        profile_store: &ProfileStore,
        config_store: &ConfigStore,
    ) -> Result<()> {
        let backup_root = self.backups_dir().join(backup_id);
        if !backup_root.is_dir() {
            bail!("no backup found with id '{}'", backup_id);
        }

        let mut restored = 0usize;

        for tool in Tool::ALL {
            let tool_path = backup_root.join(tool.dir_name());
            if !tool_path.is_dir() {
                continue;
            }
            for profile_entry in fs::read_dir(&tool_path)? {
                let profile_entry = profile_entry?;
                let profile_path = profile_entry.path();
                if !profile_path.is_dir() || profile_path.is_symlink() {
                    continue;
                }
                let profile_name = match profile_path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_owned(),
                    None => continue,
                };

                let dest_dir = profile_store.profile_dir(tool, &profile_name);
                fs::create_dir_all(&dest_dir).with_context(|| {
                    format!("could not create profile dir {}", dest_dir.display())
                })?;

                let profile_meta =
                    restore_profile_meta(config_store, tool, &profile_name, &profile_path)?;
                profile_meta.credential_backend.validate_for_tool(tool)?;
                config_store.upsert_profile(tool, &profile_name, profile_meta.clone())?;

                if profile_meta.credential_backend == CredentialBackend::SystemKeyring {
                    secure_store::restore_profile_secret(tool, &profile_name, backup_id)?;
                    restored += 1;
                }

                for file_entry in fs::read_dir(&profile_path)? {
                    let file_entry = file_entry?;
                    let src = file_entry.path();
                    if src.is_symlink() || !src.is_file() {
                        continue;
                    }
                    if file_entry.file_name() == METADATA_FILE {
                        continue;
                    }
                    let dst = dest_dir.join(file_entry.file_name());
                    fs::copy(&src, &dst).with_context(|| {
                        format!("could not restore {} to {}", src.display(), dst.display())
                    })?;
                    set_permissions_600(&dst)?;
                    restored += 1;
                }
            }
        }

        if restored == 0 {
            bail!(
                "backup '{}' exists but contains no files to restore",
                backup_id
            );
        }

        Ok(())
    }

    /// Remove oldest backup entries beyond `max_backups`.
    pub fn prune(&self, max_backups: usize) -> Result<()> {
        let base = self.backups_dir();
        if !base.exists() {
            return Ok(());
        }

        let mut backup_ids: Vec<String> = fs::read_dir(&base)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir() && !e.path().is_symlink())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();

        // Sort newest-first; remove from the tail.
        backup_ids.sort_by(|a, b| b.cmp(a));

        for old in backup_ids.into_iter().skip(max_backups) {
            let path = base.join(&old);
            for tool in Tool::ALL {
                let tool_path = path.join(tool.dir_name());
                if !tool_path.is_dir() {
                    continue;
                }
                for profile_entry in fs::read_dir(&tool_path)? {
                    let profile_entry = profile_entry?;
                    let profile_path = profile_entry.path();
                    if !profile_path.is_dir() || profile_path.is_symlink() {
                        continue;
                    }
                    if let Some(profile_name) =
                        profile_path.file_name().and_then(|name| name.to_str())
                    {
                        let _ = secure_store::delete_backup_secret(tool, profile_name, &old);
                    }
                }
            }
            fs::remove_dir_all(&path)
                .with_context(|| format!("could not remove old backup {}", path.display()))?;
        }

        Ok(())
    }
}

fn backup_id_now() -> String {
    // Filesystem-safe, lexicographically sortable, and unique even when multiple
    // snapshots are created within the same wall-clock tick in one process.
    let seq = BACKUP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{:04}",
        Utc::now().format("%Y-%m-%dT%H-%M-%S%.3fZ"),
        seq % 10_000
    )
}

fn write_metadata(path: &Path, metadata: &BackupProfileMetadata) -> Result<()> {
    let json =
        serde_json::to_vec_pretty(metadata).context("could not serialize backup metadata")?;
    fs::write(path, json)
        .with_context(|| format!("could not write backup metadata {}", path.display()))?;
    set_permissions_600(path)
}

fn read_metadata(path: &Path) -> Result<BackupProfileMetadata> {
    let bytes = fs::read(path).with_context(|| {
        format!(
            "backup is missing required metadata file {}",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("could not parse backup metadata file {}", path.display()))
}

fn restore_profile_meta(
    config_store: &ConfigStore,
    tool: Tool,
    profile_name: &str,
    profile_path: &Path,
) -> Result<ProfileMeta> {
    let metadata_path = profile_path.join(METADATA_FILE);
    if metadata_path.is_file() {
        return Ok(read_metadata(&metadata_path)?.profile_meta);
    }

    if let Some(existing) = existing_profile_meta(config_store, tool, profile_name)? {
        return Ok(existing);
    }

    Ok(ProfileMeta {
        added_at: Utc::now(),
        auth_method: infer_auth_method(tool, profile_path)?,
        credential_backend: CredentialBackend::File,
        label: None,
    })
}

fn existing_profile_meta(
    config_store: &ConfigStore,
    tool: Tool,
    profile_name: &str,
) -> Result<Option<ProfileMeta>> {
    let config = config_store.load()?;
    let existing = config.profiles_for(tool).get(profile_name);
    Ok(existing.cloned())
}

fn infer_auth_method(tool: Tool, profile_path: &Path) -> Result<AuthMethod> {
    match tool {
        Tool::Claude => infer_json_field_auth_method(
            &profile_path.join(".credentials.json"),
            "apiKey",
            AuthMethod::ApiKey,
            AuthMethod::OAuth,
        ),
        Tool::Codex => infer_json_field_auth_method(
            &profile_path.join("auth.json"),
            "token",
            AuthMethod::ApiKey,
            AuthMethod::OAuth,
        ),
        Tool::Gemini => {
            if profile_path.join(".env").is_file() {
                Ok(AuthMethod::ApiKey)
            } else {
                Ok(AuthMethod::OAuth)
            }
        }
    }
}

fn infer_json_field_auth_method(
    path: &Path,
    api_field: &str,
    api_method: AuthMethod,
    fallback_method: AuthMethod,
) -> Result<AuthMethod> {
    let bytes = fs::read(path)
        .with_context(|| format!("could not read legacy backup file {}", path.display()))?;
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("could not parse legacy backup file {}", path.display()))?;
    Ok(if json.get(api_field).and_then(|v| v.as_str()).is_some() {
        api_method
    } else {
        fallback_method
    })
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    use super::*;
    use crate::auth::secure_store;

    fn manager(dir: &Path) -> BackupManager {
        BackupManager::new(dir)
    }

    fn profile_store(dir: &Path) -> ProfileStore {
        ProfileStore::new(dir)
    }

    fn make_profile(dir: &Path, tool: Tool, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let store = profile_store(dir);
        store.create(tool, name).unwrap();
        for (filename, contents) in files {
            store.write_file(tool, name, filename, contents).unwrap();
        }
        store.profile_dir(tool, name)
    }

    fn profile_meta() -> ProfileMeta {
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: crate::config::AuthMethod::ApiKey,
            credential_backend: CredentialBackend::File,
            label: Some("Test".to_owned()),
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    fn write_security_mock(bin: &Path) {
        fs::write(
            bin,
            "#!/bin/sh\n\
             cmd=\"$1\"\n\
             shift\n\
             case \"$cmd\" in\n\
               find-generic-password)\n\
                 service=''\n\
                 account=''\n\
                 while [ \"$#\" -gt 0 ]; do\n\
                   case \"$1\" in\n\
                     -s) shift; service=\"$1\" ;;\n\
                     -a) shift; account=\"$1\" ;;\n\
                   esac\n\
                   shift\n\
                 done\n\
                 key=$(printf '%s' \"$service-$account\" | tr ' /:' '___')\n\
                 store=\"$HOME/$key.json\"\n\
                 if [ -f \"$store\" ]; then cat \"$store\"; exit 0; fi\n\
                 echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
                 exit 44\n\
                 ;;\n\
               add-generic-password)\n\
                 service=''\n\
                 account=''\n\
                 secret=''\n\
                 while [ \"$#\" -gt 0 ]; do\n\
                   case \"$1\" in\n\
                     -s) shift; service=\"$1\" ;;\n\
                     -a) shift; account=\"$1\" ;;\n\
                     -w)\n\
                       shift\n\
                       if [ \"$#\" -gt 0 ] && [ \"${1#-}\" = \"$1\" ]; then\n\
                         secret=\"$1\"\n\
                       else\n\
                         IFS= read -r secret || true\n\
                         continue\n\
                       fi\n\
                       ;;\n\
                   esac\n\
                   shift\n\
                 done\n\
                 key=$(printf '%s' \"$service-$account\" | tr ' /:' '___')\n\
                 printf '%s' \"$secret\" > \"$HOME/$key.json\"\n\
                 exit 0\n\
                 ;;\n\
               delete-generic-password)\n\
                 service=''\n\
                 account=''\n\
                 while [ \"$#\" -gt 0 ]; do\n\
                   case \"$1\" in\n\
                     -s) shift; service=\"$1\" ;;\n\
                     -a) shift; account=\"$1\" ;;\n\
                   esac\n\
                   shift\n\
                 done\n\
                 key=$(printf '%s' \"$service-$account\" | tr ' /:' '___')\n\
                 rm -f \"$HOME/$key.json\"\n\
                 exit 0\n\
                 ;;\n\
             esac\n\
             exit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        fs::set_permissions(bin, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn write_legacy_backup(
        dir: &Path,
        backup_id: &str,
        tool: Tool,
        name: &str,
        files: &[(&str, &[u8])],
    ) -> PathBuf {
        let backup_dir = dir
            .join(BACKUPS_DIR)
            .join(backup_id)
            .join(tool.dir_name())
            .join(name);
        fs::create_dir_all(&backup_dir).unwrap();
        for (filename, contents) in files {
            fs::write(backup_dir.join(filename), contents).unwrap();
        }
        backup_dir
    }

    #[test]
    fn list_empty_when_no_backups_dir() {
        let dir = tempdir().unwrap();
        let m = manager(dir.path());
        let entries = m.list().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn snapshot_creates_backup_with_files() {
        let dir = tempdir().unwrap();
        let profile_dir = make_profile(
            dir.path(),
            Tool::Claude,
            "work",
            &[(".credentials.json", b"{\"apiKey\":\"sk-ant-test\"}")],
        );

        let m = manager(dir.path());
        let backup_path = m
            .snapshot(Tool::Claude, "work", &profile_dir, &profile_meta())
            .unwrap();

        assert!(backup_path.is_dir());
        assert!(backup_path.join(".credentials.json").exists());
        assert!(backup_path.join(METADATA_FILE).exists());
    }

    #[test]
    #[cfg(unix)]
    fn snapshot_enforces_600_on_backup_files() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let profile_dir = make_profile(
            dir.path(),
            Tool::Claude,
            "work",
            &[("secret.json", b"data")],
        );

        let m = manager(dir.path());
        let backup_path = m
            .snapshot(Tool::Claude, "work", &profile_dir, &profile_meta())
            .unwrap();

        let mode = fs::metadata(backup_path.join("secret.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn snapshot_skips_symlinks() {
        let dir = tempdir().unwrap();
        let profile_dir = make_profile(dir.path(), Tool::Claude, "work", &[("real.json", b"data")]);

        // Create a symlink inside the profile dir.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                profile_dir.join("real.json"),
                profile_dir.join("link.json"),
            )
            .unwrap();
        }

        let m = manager(dir.path());
        let backup_path = m
            .snapshot(Tool::Claude, "work", &profile_dir, &profile_meta())
            .unwrap();

        assert!(backup_path.join("real.json").exists());
        assert!(!backup_path.join("link.json").exists());
    }

    #[test]
    fn list_returns_entries_newest_first() {
        let dir = tempdir().unwrap();
        let profile_dir = make_profile(dir.path(), Tool::Claude, "work", &[("f.json", b"x")]);

        let m = manager(dir.path());
        // Back-to-back snapshots still get distinct, sortable ids.
        m.snapshot(Tool::Claude, "work", &profile_dir, &profile_meta())
            .unwrap();
        m.snapshot(Tool::Claude, "work", &profile_dir, &profile_meta())
            .unwrap();

        let entries = m.list().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            entries[0].backup_id > entries[1].backup_id,
            "should be newest-first"
        );
    }

    #[test]
    fn list_covers_all_tools() {
        let dir = tempdir().unwrap();
        let claude_dir = make_profile(dir.path(), Tool::Claude, "work", &[("c.json", b"c")]);
        let codex_dir = make_profile(dir.path(), Tool::Codex, "main", &[("a.json", b"a")]);

        let m = manager(dir.path());
        m.snapshot(Tool::Claude, "work", &claude_dir, &profile_meta())
            .unwrap();
        m.snapshot(Tool::Codex, "main", &codex_dir, &profile_meta())
            .unwrap();

        let entries = m.list().unwrap();
        assert_eq!(entries.len(), 2);
        let tools: Vec<Tool> = entries.iter().map(|e| e.tool).collect();
        assert!(tools.contains(&Tool::Claude));
        assert!(tools.contains(&Tool::Codex));
    }

    #[test]
    fn restore_missing_id_errors() {
        let dir = tempdir().unwrap();
        let m = manager(dir.path());
        let ps = profile_store(dir.path());
        let cs = ConfigStore::new(dir.path());
        let err = m
            .restore("2099-01-01T00-00-00.000Z-0000", &ps, &cs)
            .unwrap_err();
        assert!(err.to_string().contains("no backup found"));
    }

    #[test]
    fn restore_writes_files_into_profile_dir() {
        let dir = tempdir().unwrap();
        let profile_dir = make_profile(
            dir.path(),
            Tool::Claude,
            "work",
            &[(".credentials.json", b"{\"apiKey\":\"sk-ant-orig\"}")],
        );

        let m = manager(dir.path());
        m.snapshot(Tool::Claude, "work", &profile_dir, &profile_meta())
            .unwrap();

        // Overwrite the profile file to simulate a change.
        let ps = profile_store(dir.path());
        ps.write_file(Tool::Claude, "work", ".credentials.json", b"changed")
            .unwrap();

        let entries = m.list().unwrap();
        let cs = ConfigStore::new(dir.path());
        m.restore(&entries[0].backup_id, &ps, &cs).unwrap();

        let restored = ps
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        assert_eq!(restored, b"{\"apiKey\":\"sk-ant-orig\"}");
    }

    #[test]
    fn restore_by_id_restores_only_one_backup_target() {
        let dir = tempdir().unwrap();
        let claude_dir = make_profile(dir.path(), Tool::Claude, "work", &[("c.json", b"claude")]);
        let codex_dir = make_profile(dir.path(), Tool::Codex, "main", &[("a.json", b"codex")]);

        let m = manager(dir.path());
        m.snapshot(Tool::Claude, "work", &claude_dir, &profile_meta())
            .unwrap();
        m.snapshot(Tool::Codex, "main", &codex_dir, &profile_meta())
            .unwrap();

        let ps = profile_store(dir.path());
        ps.write_file(Tool::Claude, "work", "c.json", b"changed-claude")
            .unwrap();
        ps.write_file(Tool::Codex, "main", "a.json", b"changed-codex")
            .unwrap();

        let entries = m.list().unwrap();
        let claude_id = entries
            .iter()
            .find(|e| e.tool == Tool::Claude && e.profile == "work")
            .unwrap()
            .backup_id
            .clone();

        let cs = ConfigStore::new(dir.path());
        m.restore(&claude_id, &ps, &cs).unwrap();

        assert_eq!(
            ps.read_file(Tool::Claude, "work", "c.json").unwrap(),
            b"claude"
        );
        assert_eq!(
            ps.read_file(Tool::Codex, "main", "a.json").unwrap(),
            b"changed-codex"
        );
    }

    #[test]
    fn prune_removes_oldest_entries() {
        let dir = tempdir().unwrap();
        let profile_dir = make_profile(dir.path(), Tool::Claude, "work", &[("f.json", b"x")]);
        let m = manager(dir.path());

        for _ in 0..3 {
            m.snapshot(Tool::Claude, "work", &profile_dir, &profile_meta())
                .unwrap();
        }

        let entries = m.list().unwrap();
        let unique_ids: std::collections::HashSet<_> =
            entries.iter().map(|e| e.backup_id.clone()).collect();
        assert_eq!(
            unique_ids.len(),
            3,
            "backup ids must be unique per snapshot"
        );

        assert_eq!(entries.len(), 3);
        m.prune(2).unwrap();
        assert_eq!(m.list().unwrap().len(), 2);
    }

    #[test]
    fn prune_noop_when_under_limit() {
        let dir = tempdir().unwrap();
        let profile_dir = make_profile(dir.path(), Tool::Claude, "work", &[("f.json", b"x")]);
        let m = manager(dir.path());
        m.snapshot(Tool::Claude, "work", &profile_dir, &profile_meta())
            .unwrap();

        m.prune(10).unwrap();
        assert_eq!(m.list().unwrap().len(), 1);
    }

    #[test]
    fn restore_recreates_missing_config_entry() {
        let dir = tempdir().unwrap();
        let meta = profile_meta();
        let profile_dir = make_profile(
            dir.path(),
            Tool::Claude,
            "work",
            &[(".credentials.json", b"{\"apiKey\":\"sk-ant-orig\"}")],
        );

        let m = manager(dir.path());
        m.snapshot(Tool::Claude, "work", &profile_dir, &meta)
            .unwrap();

        let ps = profile_store(dir.path());
        ps.delete(Tool::Claude, "work").unwrap();

        let cs = ConfigStore::new(dir.path());
        let backup_id = m.list().unwrap()[0].backup_id.clone();
        m.restore(&backup_id, &ps, &cs).unwrap();

        let config = cs.load().unwrap();
        let restored = &config.profiles_for(Tool::Claude)["work"];
        assert_eq!(restored.auth_method, meta.auth_method);
        assert_eq!(restored.label, meta.label);
    }

    #[test]
    fn restore_legacy_claude_backup_infers_api_key_profile() {
        let dir = tempdir().unwrap();
        write_legacy_backup(
            dir.path(),
            "legacy-claude",
            Tool::Claude,
            "work",
            &[(
                ".credentials.json",
                b"{\"apiKey\":\"sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"}",
            )],
        );

        let m = manager(dir.path());
        let ps = profile_store(dir.path());
        let cs = ConfigStore::new(dir.path());
        m.restore("legacy-claude", &ps, &cs).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].auth_method,
            AuthMethod::ApiKey
        );
        assert_eq!(
            ps.read_file(Tool::Claude, "work", ".credentials.json")
                .unwrap(),
            b"{\"apiKey\":\"sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"}"
        );
    }

    #[test]
    fn restore_legacy_codex_backup_infers_api_key_profile() {
        let dir = tempdir().unwrap();
        write_legacy_backup(
            dir.path(),
            "legacy-codex",
            Tool::Codex,
            "main",
            &[
                ("auth.json", b"{\"token\":\"sk-codex-test-key-12345\"}"),
                ("config.toml", b"cli_auth_credentials_store = \"file\"\n"),
            ],
        );

        let m = manager(dir.path());
        let ps = profile_store(dir.path());
        let cs = ConfigStore::new(dir.path());
        m.restore("legacy-codex", &ps, &cs).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Codex)["main"].auth_method,
            AuthMethod::ApiKey
        );
        assert_eq!(
            ps.read_file(Tool::Codex, "main", "auth.json").unwrap(),
            b"{\"token\":\"sk-codex-test-key-12345\"}"
        );
    }

    #[test]
    fn restore_legacy_gemini_backup_infers_api_key_profile() {
        let dir = tempdir().unwrap();
        write_legacy_backup(
            dir.path(),
            "legacy-gemini",
            Tool::Gemini,
            "default",
            &[(".env", b"GEMINI_API_KEY=AIzaLegacy\n")],
        );

        let m = manager(dir.path());
        let ps = profile_store(dir.path());
        let cs = ConfigStore::new(dir.path());
        m.restore("legacy-gemini", &ps, &cs).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Gemini)["default"].auth_method,
            AuthMethod::ApiKey
        );
        assert_eq!(
            ps.read_file(Tool::Gemini, "default", ".env").unwrap(),
            b"GEMINI_API_KEY=AIzaLegacy\n"
        );
    }

    #[test]
    fn snapshot_and_restore_secure_profile_secret() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let security_bin = bin_dir.join("security");
        write_security_mock(&security_bin);
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));
        let _security = EnvVarGuard::set(
            "AISW_SECURITY_BIN",
            security_bin
                .to_str()
                .expect("security path should be utf-8"),
        );

        let ps = profile_store(dir.path());
        let cs = ConfigStore::new(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        secure_store::write_profile_secret(Tool::Claude, "work", br#"{"token":"tok"}"#).unwrap();
        let meta = ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            credential_backend: CredentialBackend::SystemKeyring,
            label: None,
        };
        cs.add_profile(Tool::Claude, "work", meta.clone()).unwrap();

        let profile_dir = ps.profile_dir(Tool::Claude, "work");
        let backup_path = manager(dir.path())
            .snapshot(Tool::Claude, "work", &profile_dir, &meta)
            .unwrap();
        let backup_id = backup_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|p| p.to_str())
            .unwrap()
            .to_owned();

        secure_store::delete_profile_secret(Tool::Claude, "work").unwrap();
        manager(dir.path()).restore(&backup_id, &ps, &cs).unwrap();

        assert_eq!(
            secure_store::read_profile_secret(Tool::Claude, "work")
                .unwrap()
                .as_deref(),
            Some(br#"{"token":"tok"}"#.as_slice())
        );
    }
}
