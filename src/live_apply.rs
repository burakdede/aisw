use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, bail, Context, Result};

#[derive(Debug)]
pub enum LiveFileChange {
    Write { path: PathBuf, contents: Vec<u8> },
    Delete { path: PathBuf },
}

impl LiveFileChange {
    pub fn write(path: PathBuf, contents: Vec<u8>) -> Self {
        Self::Write { path, contents }
    }

    pub fn delete(path: PathBuf) -> Self {
        Self::Delete { path }
    }

    fn path(&self) -> &Path {
        match self {
            Self::Write { path, .. } | Self::Delete { path } => path,
        }
    }
}

enum PreparedChange {
    Write { path: PathBuf, staged_path: PathBuf },
    Delete { path: PathBuf },
}

enum OriginalFileState {
    Absent,
    Present {
        contents: Vec<u8>,
        #[cfg(unix)]
        mode: u32,
    },
}

pub fn apply_transaction(changes: Vec<LiveFileChange>) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }

    let mut seen_paths = HashSet::new();
    for change in &changes {
        let path = change.path();
        if !seen_paths.insert(path.to_path_buf()) {
            bail!(
                "live apply transaction includes duplicate target '{}'",
                path.display()
            );
        }
    }

    let snapshots = snapshot_original_state(&changes)?;
    let prepared = stage_changes(changes)?;

    let result = commit_changes(&prepared).inspect_err(|_| {
        let _ = rollback_changes(&prepared, &snapshots);
    });

    cleanup_staged_files(&prepared)?;
    result
}

fn snapshot_original_state(
    changes: &[LiveFileChange],
) -> Result<HashMap<PathBuf, OriginalFileState>> {
    let mut snapshots = HashMap::new();
    for change in changes {
        snapshots.insert(
            change.path().to_path_buf(),
            read_original_state(change.path())
                .with_context(|| format!("could not snapshot {}", change.path().display()))?,
        );
    }
    Ok(snapshots)
}

fn read_original_state(path: &Path) -> Result<OriginalFileState> {
    if !path.exists() {
        return Ok(OriginalFileState::Absent);
    }
    if path.is_symlink() {
        bail!("refusing to modify symlink target '{}'", path.display());
    }
    if !path.is_file() {
        bail!("refusing to modify non-file '{}'", path.display());
    }

    let contents =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(path)
            .with_context(|| format!("could not stat {}", path.display()))?
            .permissions()
            .mode()
            & 0o777;
        Ok(OriginalFileState::Present { contents, mode })
    }
    #[cfg(not(unix))]
    {
        Ok(OriginalFileState::Present { contents })
    }
}

fn stage_changes(changes: Vec<LiveFileChange>) -> Result<Vec<PreparedChange>> {
    let mut prepared = Vec::with_capacity(changes.len());
    for change in changes {
        match change {
            LiveFileChange::Write { path, contents } => {
                let parent = path.parent().ok_or_else(|| {
                    anyhow!(
                        "live apply target '{}' has no parent directory",
                        path.display()
                    )
                })?;
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("could not create {}", parent.display()))?;

                let staged_path = staged_path_for(&path);
                maybe_inject_fault("live_apply.stage_write")?;
                std::fs::write(&staged_path, &contents)
                    .with_context(|| format!("could not write {}", staged_path.display()))?;
                set_permissions_600(&staged_path)?;
                prepared.push(PreparedChange::Write { path, staged_path });
            }
            LiveFileChange::Delete { path } => prepared.push(PreparedChange::Delete { path }),
        }
    }
    Ok(prepared)
}

fn commit_changes(prepared: &[PreparedChange]) -> Result<()> {
    for change in prepared {
        match change {
            PreparedChange::Write { path, staged_path } => {
                maybe_inject_fault("live_apply.commit_write")?;
                std::fs::rename(staged_path, path).with_context(|| {
                    format!(
                        "could not replace {} with {}",
                        path.display(),
                        staged_path.display()
                    )
                })?;
            }
            PreparedChange::Delete { path } => {
                maybe_inject_fault("live_apply.commit_delete")?;
                if path.exists() {
                    std::fs::remove_file(path)
                        .with_context(|| format!("could not remove {}", path.display()))?;
                }
            }
        }
    }
    Ok(())
}

fn rollback_changes(
    prepared: &[PreparedChange],
    snapshots: &HashMap<PathBuf, OriginalFileState>,
) -> Result<()> {
    for change in prepared.iter().rev() {
        let path = match change {
            PreparedChange::Write { path, .. } | PreparedChange::Delete { path } => path,
        };
        let Some(snapshot) = snapshots.get(path) else {
            continue;
        };
        restore_original_state(path, snapshot)
            .with_context(|| format!("could not restore {}", path.display()))?;
    }
    Ok(())
}

fn restore_original_state(path: &Path, snapshot: &OriginalFileState) -> Result<()> {
    match snapshot {
        OriginalFileState::Absent => {
            if path.exists() {
                std::fs::remove_file(path)
                    .with_context(|| format!("could not remove {}", path.display()))?;
            }
        }
        OriginalFileState::Present {
            contents,
            #[cfg(unix)]
            mode,
        } => {
            let parent = path.parent().ok_or_else(|| {
                anyhow!(
                    "live apply target '{}' has no parent directory",
                    path.display()
                )
            })?;
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
            std::fs::write(path, contents)
                .with_context(|| format!("could not write {}", path.display()))?;
            #[cfg(unix)]
            set_permissions_mode(path, *mode)?;
            #[cfg(not(unix))]
            set_permissions_600(path)?;
        }
    }
    Ok(())
}

fn cleanup_staged_files(prepared: &[PreparedChange]) -> Result<()> {
    for change in prepared {
        if let PreparedChange::Write { staged_path, .. } = change {
            if staged_path.exists() {
                std::fs::remove_file(staged_path).with_context(|| {
                    format!("could not clean up staged file {}", staged_path.display())
                })?;
            }
        }
    }
    Ok(())
}

fn staged_path_for(dest: &Path) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let parent = dest.parent().expect("validated parent directory");
    let file_name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("live");
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    parent.join(format!(
        ".{file_name}.aisw-stage-{}-{id}",
        std::process::id()
    ))
}

#[cfg(unix)]
fn set_permissions_600(path: &Path) -> Result<()> {
    set_permissions_mode(path, 0o600)
}

#[cfg(not(unix))]
fn set_permissions_600(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_permissions_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("could not set permissions on {}", path.display()))
}

fn maybe_inject_fault(label: &str) -> Result<()> {
    let Some(rule) = active_fault_rule() else {
        return Ok(());
    };
    if rule.label != label {
        return Ok(());
    }

    let mut hits = fault_hits()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let hit = hits.entry(rule.label.clone()).or_insert(0);
    *hit += 1;
    if *hit == rule.fail_on_hit {
        bail!("injected live-apply failure at {}", label);
    }
    Ok(())
}

fn active_fault_rule() -> Option<FaultRule> {
    parse_fault_rule(std::env::var("AISW_FAULT_INJECTION").ok()?.as_str())
}

fn parse_fault_rule(raw: &str) -> Option<FaultRule> {
    let (label, count) = match raw.split_once(':') {
        Some((label, count)) => (label, count.parse().ok()?),
        None => (raw, 1),
    };
    if label.is_empty() || count == 0 {
        return None;
    }
    Some(FaultRule {
        label: label.to_owned(),
        fail_on_hit: count,
    })
}

fn fault_hits() -> &'static Mutex<HashMap<String, usize>> {
    static HITS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    HITS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct FaultRule {
    label: String,
    fail_on_hit: usize,
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use tempfile::tempdir;

    use super::*;

    fn fault_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn reset_fault_state() {
        let mut hits = fault_hits()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        hits.clear();
        // SAFETY: fault-injection tests serialize all environment mutation through
        // `fault_env_lock`, so there is no concurrent env access within this process.
        unsafe {
            std::env::remove_var("AISW_FAULT_INJECTION");
        }
    }

    #[test]
    fn rejects_duplicate_targets_in_one_transaction() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("auth.json");

        let err = apply_transaction(vec![
            LiveFileChange::write(target.clone(), b"one".to_vec()),
            LiveFileChange::write(target.clone(), b"two".to_vec()),
        ])
        .unwrap_err();

        assert!(err.to_string().contains("duplicate target"));
    }

    #[test]
    #[cfg(unix)]
    fn refuses_to_modify_symlink_targets() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("auth.json");
        let symlink_path = dir.path().join("auth-link.json");
        std::fs::write(&target, "token").unwrap();
        std::os::unix::fs::symlink(&target, &symlink_path).unwrap();

        let err = apply_transaction(vec![LiveFileChange::write(
            symlink_path.clone(),
            b"new-token".to_vec(),
        )])
        .unwrap_err();

        assert!(!err.to_string().is_empty());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "token");
    }

    #[test]
    fn delete_rollback_restores_original_contents() {
        let _guard = fault_env_lock().lock().unwrap();
        reset_fault_state();

        let dir = tempdir().unwrap();
        let target = dir.path().join("state.json");
        std::fs::write(&target, "original").unwrap();

        // SAFETY: serialized by `fault_env_lock`.
        unsafe {
            std::env::set_var("AISW_FAULT_INJECTION", "live_apply.commit_delete");
        }

        let err = apply_transaction(vec![LiveFileChange::delete(target.clone())]).unwrap_err();
        assert!(err.to_string().contains("injected live-apply failure"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "original");

        reset_fault_state();
    }

    #[test]
    #[cfg(unix)]
    fn rollback_restores_previous_contents_and_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = fault_env_lock().lock().unwrap();
        reset_fault_state();

        let dir = tempdir().unwrap();
        let first = dir.path().join("auth.json");
        let second = dir.path().join("config.toml");
        std::fs::write(&first, "old-auth").unwrap();
        std::fs::write(&second, "old-config").unwrap();
        std::fs::set_permissions(&first, std::fs::Permissions::from_mode(0o640)).unwrap();

        // SAFETY: serialized by `fault_env_lock`.
        unsafe {
            std::env::set_var("AISW_FAULT_INJECTION", "live_apply.commit_write:2");
        }

        let err = apply_transaction(vec![
            LiveFileChange::write(first.clone(), b"new-auth".to_vec()),
            LiveFileChange::write(second.clone(), b"new-config".to_vec()),
        ])
        .unwrap_err();

        assert!(err.to_string().contains("injected live-apply failure"));
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "old-auth");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "old-config");
        let mode = std::fs::metadata(&first).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);

        reset_fault_state();
    }

    #[test]
    fn cleans_up_staged_files_after_failed_write() {
        let _guard = fault_env_lock().lock().unwrap();
        reset_fault_state();

        let dir = tempdir().unwrap();
        let target = dir.path().join("auth.json");

        // SAFETY: serialized by `fault_env_lock`.
        unsafe {
            std::env::set_var("AISW_FAULT_INJECTION", "live_apply.commit_write");
        }

        let err = apply_transaction(vec![LiveFileChange::write(
            target.clone(),
            b"new-auth".to_vec(),
        )])
        .unwrap_err();
        assert!(err.to_string().contains("injected live-apply failure"));
        assert!(!target.exists());

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            entries.iter().all(|name| !name.contains(".aisw-stage-")),
            "staged files should be removed after failure: {entries:?}"
        );

        reset_fault_state();
    }
}
