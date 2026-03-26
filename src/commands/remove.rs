use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::backup::BackupManager;
use crate::cli::RemoveArgs;
use crate::config::{Config, ConfigStore};
use crate::output;
use crate::profile::ProfileStore;
use crate::types::Tool;

pub fn run(args: RemoveArgs, home: &Path) -> Result<()> {
    if !args.yes {
        // Validate before prompting — better UX to fail fast on invalid ops.
        precheck(&args, home)?;
        eprint!(
            "Remove {} profile '{}'? This cannot be undone. [y/N] ",
            args.tool, args.profile_name
        );
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .context("could not read confirmation from stdin")?;
        if !matches!(line.trim(), "y" | "Y") {
            output::print_warning("Aborted.");
            return Ok(());
        }
    }
    run_inner(args, home, true)
}

/// Entry point for non-interactive use (tests and `--yes` flag).
pub(crate) fn run_inner(args: RemoveArgs, home: &Path, confirmed: bool) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;

    if !profile_store.exists(args.tool, &args.profile_name) {
        bail!(
            "profile '{}' not found for {}.\n  \
             Run 'aisw list {}' to see available profiles.",
            args.profile_name,
            args.tool,
            args.tool
        );
    }

    let is_active = active_for(&config, args.tool) == Some(args.profile_name.as_str());
    if is_active && !args.force {
        bail!(
            "profile '{}' is currently active. \
             Switch to another profile first, or use --force.",
            args.profile_name
        );
    }

    if !confirmed {
        output::print_warning("Aborted.");
        return Ok(());
    }

    // Final backup before deleting.
    let profile_dir = profile_store.profile_dir(args.tool, &args.profile_name);
    let profile_meta = match args.tool {
        Tool::Claude => config.profiles.claude.get(&args.profile_name),
        Tool::Codex => config.profiles.codex.get(&args.profile_name),
        Tool::Gemini => config.profiles.gemini.get(&args.profile_name),
    }
    .with_context(|| {
        format!(
            "profile '{}' exists on disk for {} but is missing from config",
            args.profile_name, args.tool
        )
    })?;
    BackupManager::new(home).snapshot(args.tool, &args.profile_name, &profile_dir, profile_meta)?;

    profile_store.delete(args.tool, &args.profile_name)?;
    config_store.remove_profile(args.tool, &args.profile_name)?;

    if is_active {
        config_store.clear_active(args.tool)?;
    }

    output::print_title("Removed profile");
    output::print_kv("Tool", args.tool.display_name());
    output::print_kv("Profile", &args.profile_name);
    output::print_kv("Was active", if is_active { "yes" } else { "no" });
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Stored profile files deleted.");
    output::print_effect("Backup created before deletion.");
    if is_active {
        output::print_effect("Active profile cleared.");
    }
    output::print_blank_line();
    output::print_next_step("Run 'aisw list' to review remaining profiles.");
    Ok(())
}

fn precheck(args: &RemoveArgs, home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    if !profile_store.exists(args.tool, &args.profile_name) {
        bail!(
            "profile '{}' not found for {}.\n  \
             Run 'aisw list {}' to see available profiles.",
            args.profile_name,
            args.tool,
            args.tool
        );
    }
    let config = ConfigStore::new(home).load()?;
    let is_active = active_for(&config, args.tool) == Some(args.profile_name.as_str());
    if is_active && !args.force {
        bail!(
            "profile '{}' is currently active. \
             Switch to another profile first, or use --force.",
            args.profile_name
        );
    }
    Ok(())
}

fn active_for(config: &Config, tool: Tool) -> Option<&str> {
    match tool {
        Tool::Claude => config.active.claude.as_deref(),
        Tool::Codex => config.active.codex.as_deref(),
        Tool::Gemini => config.active.gemini.as_deref(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::cli::RemoveArgs;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    fn claude_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn add_claude(home: &std::path::Path, name: &str) {
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        auth::claude::add_api_key(&ps, &cs, name, claude_key(), None).unwrap();
    }

    fn remove_args(tool: Tool, name: &str, yes: bool, force: bool) -> RemoveArgs {
        RemoveArgs {
            tool,
            profile_name: name.to_owned(),
            yes,
            force,
        }
    }

    #[test]
    fn removes_profile_when_confirmed() {
        let tmp = tempdir().unwrap();
        add_claude(tmp.path(), "work");

        run_inner(
            remove_args(Tool::Claude, "work", true, false),
            tmp.path(),
            true,
        )
        .unwrap();

        let ps = ProfileStore::new(tmp.path());
        assert!(!ps.exists(Tool::Claude, "work"));

        let config = ConfigStore::new(tmp.path()).load().unwrap();
        assert!(!config.profiles.claude.contains_key("work"));
    }

    #[test]
    fn nonexistent_profile_errors() {
        let tmp = tempdir().unwrap();
        let err = run_inner(
            remove_args(Tool::Claude, "ghost", true, false),
            tmp.path(),
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn active_profile_blocked_without_force() {
        let tmp = tempdir().unwrap();
        add_claude(tmp.path(), "work");
        ConfigStore::new(tmp.path())
            .set_active(Tool::Claude, "work")
            .unwrap();

        let err = run_inner(
            remove_args(Tool::Claude, "work", true, false),
            tmp.path(),
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("currently active"));
    }

    #[test]
    fn active_profile_removed_with_force_and_active_cleared() {
        let tmp = tempdir().unwrap();
        add_claude(tmp.path(), "work");
        ConfigStore::new(tmp.path())
            .set_active(Tool::Claude, "work")
            .unwrap();

        run_inner(
            remove_args(Tool::Claude, "work", true, true),
            tmp.path(),
            true,
        )
        .unwrap();

        let config = ConfigStore::new(tmp.path()).load().unwrap();
        assert!(!config.profiles.claude.contains_key("work"));
        assert!(config.active.claude.is_none());
    }

    #[test]
    fn unconfirmed_aborts_without_deleting() {
        let tmp = tempdir().unwrap();
        add_claude(tmp.path(), "work");

        run_inner(
            remove_args(Tool::Claude, "work", false, false),
            tmp.path(),
            false,
        )
        .unwrap();

        let ps = ProfileStore::new(tmp.path());
        assert!(
            ps.exists(Tool::Claude, "work"),
            "profile should still exist after abort"
        );
    }

    #[test]
    fn backup_created_before_deletion() {
        let tmp = tempdir().unwrap();
        add_claude(tmp.path(), "work");

        run_inner(
            remove_args(Tool::Claude, "work", true, false),
            tmp.path(),
            true,
        )
        .unwrap();

        let backups_dir = tmp.path().join("backups");
        assert!(backups_dir.exists());
        let entries: Vec<_> = fs::read_dir(&backups_dir).unwrap().collect();
        assert!(!entries.is_empty());
    }
}
