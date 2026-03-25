use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::backup::BackupManager;
use crate::cli::BackupCommand;
use crate::profile::ProfileStore;

pub fn run(command: BackupCommand, home: &Path) -> Result<()> {
    match command {
        BackupCommand::List => run_list(home),
        BackupCommand::Restore { backup_id, yes } => run_restore(&backup_id, yes, home),
    }
}

fn run_list(home: &Path) -> Result<()> {
    let entries = BackupManager::new(home).list()?;
    if entries.is_empty() {
        println!("No backups found. Backups are created automatically before each switch.");
        return Ok(());
    }
    println!("{:<31} {:<8} PROFILE", "BACKUP ID", "TOOL");
    for e in &entries {
        println!("{:<31} {:<8} {}", e.backup_id, e.tool, e.profile);
    }
    Ok(())
}

fn run_restore(backup_id: &str, yes: bool, home: &Path) -> Result<()> {
    let manager = BackupManager::new(home);
    let entries = manager.list()?;
    let matching: Vec<_> = entries
        .iter()
        .filter(|e| e.backup_id == backup_id)
        .collect();
    if matching.is_empty() {
        bail!(
            "no backup found with id '{}'.\n  \
             Run 'aisw backup list' to see available backups.",
            backup_id
        );
    }

    if !yes {
        let names: Vec<String> = matching
            .iter()
            .map(|e| format!("{}/{}", e.tool, e.profile))
            .collect();
        eprint!(
            "Restore {} from {}? This will overwrite the current profile files. [y/N] ",
            names.join(", "),
            backup_id
        );
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .context("could not read confirmation from stdin")?;
        if !matches!(line.trim(), "y" | "Y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    run_restore_inner(backup_id, home)
}

pub(crate) fn run_restore_inner(backup_id: &str, home: &Path) -> Result<()> {
    let manager = BackupManager::new(home);
    let profile_store = ProfileStore::new(home);

    let entries = manager.list()?;
    let matching: Vec<_> = entries
        .iter()
        .filter(|e| e.backup_id == backup_id)
        .collect();
    if matching.is_empty() {
        bail!(
            "no backup found with id '{}'.\n  \
             Run 'aisw backup list' to see available backups.",
            backup_id
        );
    }

    for e in &matching {
        println!(
            "Restoring {}/{} from backup {}...",
            e.tool, e.profile, backup_id
        );
    }
    manager.restore(backup_id, &profile_store)?;
    for e in &matching {
        println!(
            "Restored. The \"{}\" profile now has credentials from that backup.",
            e.profile
        );
        println!("Run 'aisw use {} {}' to switch to it.", e.tool, e.profile);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::backup::BackupManager;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    fn make_profile(home: &Path, tool: Tool, name: &str) {
        let ps = ProfileStore::new(home);
        ps.create(tool, name).unwrap();
        ps.write_file(tool, name, "creds.json", b"{\"key\":\"val\"}")
            .unwrap();
    }

    fn snapshot(home: &Path, tool: Tool, name: &str) -> String {
        let ps = ProfileStore::new(home);
        let profile_dir = ps.profile_dir(tool, name);
        let m = BackupManager::new(home);
        m.snapshot(tool, name, &profile_dir).unwrap();
        m.list().unwrap()[0].backup_id.clone()
    }

    #[test]
    fn list_empty_prints_no_backups_message() {
        let dir = tempdir().unwrap();
        // No error, no backups — run_list should succeed with no output (we can't
        // easily capture stdout in unit tests, but we verify it doesn't error).
        run_list(dir.path()).unwrap();
    }

    #[test]
    fn list_with_backups_does_not_error() {
        let dir = tempdir().unwrap();
        make_profile(dir.path(), Tool::Claude, "work");
        snapshot(dir.path(), Tool::Claude, "work");
        run_list(dir.path()).unwrap();
    }

    #[test]
    fn restore_inner_valid_timestamp_restores_files() {
        let dir = tempdir().unwrap();
        make_profile(dir.path(), Tool::Claude, "work");
        let ts = snapshot(dir.path(), Tool::Claude, "work");

        // Overwrite the profile file.
        let ps = ProfileStore::new(dir.path());
        ps.write_file(Tool::Claude, "work", "creds.json", b"changed")
            .unwrap();

        run_restore_inner(&ts, dir.path()).unwrap();

        let contents = ps.read_file(Tool::Claude, "work", "creds.json").unwrap();
        assert_eq!(contents, b"{\"key\":\"val\"}");
    }

    #[test]
    fn restore_inner_invalid_timestamp_errors() {
        let dir = tempdir().unwrap();
        let err = run_restore_inner("9999-99-99T00-00-00Z", dir.path()).unwrap_err();
        assert!(err.to_string().contains("no backup found"));
    }

    #[test]
    fn restore_inner_error_message_mentions_list_command() {
        let dir = tempdir().unwrap();
        let err = run_restore_inner("bad-timestamp", dir.path()).unwrap_err();
        assert!(err.to_string().contains("aisw backup list"));
    }

    #[test]
    fn run_list_empty_dir_exits_ok() {
        let dir = tempdir().unwrap();
        // backups dir does not even exist yet
        run_list(dir.path()).unwrap();
    }
}
