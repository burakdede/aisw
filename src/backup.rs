use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::profile::ProfileStore;
use crate::types::Tool;

const BACKUPS_DIR: &str = "backups";
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
    pub fn snapshot(&self, tool: Tool, name: &str, profile_dir: &Path) -> Result<PathBuf> {
        let backup_id = backup_id_now();
        let dest = self
            .backups_dir()
            .join(&backup_id)
            .join(tool.dir_name())
            .join(name);
        fs::create_dir_all(&dest)
            .with_context(|| format!("could not create backup directory {}", dest.display()))?;

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

            for tool in [Tool::Claude, Tool::Codex, Tool::Gemini] {
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
    pub fn restore(&self, backup_id: &str, profile_store: &ProfileStore) -> Result<()> {
        let backup_root = self.backups_dir().join(backup_id);
        if !backup_root.is_dir() {
            bail!("no backup found with id '{}'", backup_id);
        }

        let mut restored = 0usize;

        for tool in [Tool::Claude, Tool::Codex, Tool::Gemini] {
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

                for file_entry in fs::read_dir(&profile_path)? {
                    let file_entry = file_entry?;
                    let src = file_entry.path();
                    if src.is_symlink() || !src.is_file() {
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
    use std::fs;

    use tempfile::tempdir;

    use super::*;

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
        let backup_path = m.snapshot(Tool::Claude, "work", &profile_dir).unwrap();

        assert!(backup_path.is_dir());
        assert!(backup_path.join(".credentials.json").exists());
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
        let backup_path = m.snapshot(Tool::Claude, "work", &profile_dir).unwrap();

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
        let backup_path = m.snapshot(Tool::Claude, "work", &profile_dir).unwrap();

        assert!(backup_path.join("real.json").exists());
        assert!(!backup_path.join("link.json").exists());
    }

    #[test]
    fn list_returns_entries_newest_first() {
        let dir = tempdir().unwrap();
        let profile_dir = make_profile(dir.path(), Tool::Claude, "work", &[("f.json", b"x")]);

        let m = manager(dir.path());
        // Back-to-back snapshots still get distinct, sortable ids.
        m.snapshot(Tool::Claude, "work", &profile_dir).unwrap();
        m.snapshot(Tool::Claude, "work", &profile_dir).unwrap();

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
        m.snapshot(Tool::Claude, "work", &claude_dir).unwrap();
        m.snapshot(Tool::Codex, "main", &codex_dir).unwrap();

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
        let err = m.restore("2099-01-01T00-00-00.000Z-0000", &ps).unwrap_err();
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
        m.snapshot(Tool::Claude, "work", &profile_dir).unwrap();

        // Overwrite the profile file to simulate a change.
        let ps = profile_store(dir.path());
        ps.write_file(Tool::Claude, "work", ".credentials.json", b"changed")
            .unwrap();

        let entries = m.list().unwrap();
        m.restore(&entries[0].backup_id, &ps).unwrap();

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
        m.snapshot(Tool::Claude, "work", &claude_dir).unwrap();
        m.snapshot(Tool::Codex, "main", &codex_dir).unwrap();

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

        m.restore(&claude_id, &ps).unwrap();

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
            m.snapshot(Tool::Claude, "work", &profile_dir).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        assert_eq!(m.list().unwrap().len(), 3);
        m.prune(2).unwrap();
        assert_eq!(m.list().unwrap().len(), 2);
    }

    #[test]
    fn prune_noop_when_under_limit() {
        let dir = tempdir().unwrap();
        let profile_dir = make_profile(dir.path(), Tool::Claude, "work", &[("f.json", b"x")]);
        let m = manager(dir.path());
        m.snapshot(Tool::Claude, "work", &profile_dir).unwrap();

        m.prune(10).unwrap();
        assert_eq!(m.list().unwrap().len(), 1);
    }
}
