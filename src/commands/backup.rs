use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::backup::BackupManager;
use crate::cli::{BackupCommand, BackupListArgs};
use crate::config::ConfigStore;
use crate::output;
use crate::profile::ProfileStore;
use crate::runtime;

pub fn run(command: BackupCommand, home: &Path) -> Result<()> {
    match command {
        BackupCommand::List(args) => run_list(args, home),
        BackupCommand::Restore { backup_id, yes } => run_restore(&backup_id, yes, home),
    }
}

fn run_list(args: BackupListArgs, home: &Path) -> Result<()> {
    let entries = BackupManager::new(home).list()?;
    if args.json {
        return print_json(&entries);
    }

    if entries.is_empty() {
        output::print_title("Backups");
        output::print_empty_state(
            "No backups found. Backups are created automatically before each switch.",
        );
        return Ok(());
    }

    output::print_title("Backups");
    output::print_table_header(&[("BACKUP ID", 31), ("TOOL", 8), ("PROFILE", 0)]);
    for e in &entries {
        output::print_table_row(&[
            (e.backup_id.as_str(), 31),
            (e.tool.binary_name(), 8),
            (e.profile.as_str(), 0),
        ]);
    }
    Ok(())
}

fn print_json(entries: &[crate::backup::BackupEntry]) -> Result<()> {
    let json_rows: Vec<serde_json::Value> = entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "backup_id": entry.backup_id,
                "tool": entry.tool.binary_name(),
                "profile": entry.profile,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&json_rows)?);
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
        if runtime::is_non_interactive() {
            bail!(
                "backup restore requires confirmation.\n  \
                 Re-run with --yes, or omit --non-interactive."
            );
        }
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
            bail!("operation cancelled by user.");
        }
    }

    run_restore_inner(backup_id, home)
}

pub(crate) fn run_restore_inner(backup_id: &str, home: &Path) -> Result<()> {
    let manager = BackupManager::new(home);
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

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

    manager.restore(backup_id, &profile_store, &config_store)?;
    for e in &matching {
        output::print_title("Restored backup");
        output::print_kv("Tool", e.tool.display_name());
        output::print_kv("Profile", &e.profile);
        output::print_kv("Backup", backup_id);
        output::print_blank_line();
        output::print_effects_header();
        output::print_effect("Stored profile files restored from backup.");
        output::print_effect("Config entry recreated if it was missing.");
        output::print_blank_line();
        output::print_next_step(output::next_step_after_restore(e.tool, &e.profile));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::backup::BackupManager;
    use crate::config::{AuthMethod, ConfigStore, ProfileMeta};
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    fn make_profile(home: &Path, tool: Tool, name: &str) {
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        ps.create(tool, name).unwrap();
        ps.write_file(tool, name, "creds.json", b"{\"key\":\"val\"}")
            .unwrap();
        cs.add_profile(
            tool,
            name,
            ProfileMeta {
                added_at: chrono::Utc::now(),
                auth_method: AuthMethod::ApiKey,
                label: None,
            },
        )
        .unwrap();
    }

    fn snapshot(home: &Path, tool: Tool, name: &str) -> String {
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        let config = cs.load().unwrap();
        let profile_meta = config.profiles_for(tool).get(name).unwrap();
        let profile_dir = ps.profile_dir(tool, name);
        let m = BackupManager::new(home);
        m.snapshot(tool, name, &profile_dir, profile_meta).unwrap();
        m.list().unwrap()[0].backup_id.clone()
    }

    #[test]
    fn list_empty_prints_no_backups_message() {
        let dir = tempdir().unwrap();
        // No error, no backups — run_list should succeed with no output (we can't
        // easily capture stdout in unit tests, but we verify it doesn't error).
        run_list(BackupListArgs { json: false }, dir.path()).unwrap();
    }

    #[test]
    fn list_with_backups_does_not_error() {
        let dir = tempdir().unwrap();
        make_profile(dir.path(), Tool::Claude, "work");
        snapshot(dir.path(), Tool::Claude, "work");
        run_list(BackupListArgs { json: false }, dir.path()).unwrap();
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
        run_list(BackupListArgs { json: false }, dir.path()).unwrap();
    }
}
