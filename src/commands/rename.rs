use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::RenameArgs;
use crate::config::ConfigStore;
use crate::profile::{validate_profile_name, ProfileStore};

pub fn run(args: RenameArgs, home: &Path) -> Result<()> {
    run_inner(args, home)
}

pub(crate) fn run_inner(args: RenameArgs, home: &Path) -> Result<()> {
    validate_profile_name(&args.new_name)?;

    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    if args.old_name == args.new_name {
        anyhow::bail!(
            "profile '{}' is already named '{}'.",
            args.old_name,
            args.new_name
        );
    }

    profile_store.rename(args.tool, &args.old_name, &args.new_name)?;

    if let Err(err) = config_store.rename_profile(args.tool, &args.old_name, &args.new_name) {
        let _ = profile_store.rename(args.tool, &args.new_name, &args.old_name);
        return Err(err).context(format!(
            "rolled back profile directory rename after config update failed for {}",
            args.tool
        ));
    }

    println!(
        "Renamed {} profile '{}' to '{}'.",
        args.tool, args.old_name, args.new_name
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::config::ConfigStore;
    use crate::types::Tool;

    fn rename_args(tool: Tool, old_name: &str, new_name: &str) -> RenameArgs {
        RenameArgs {
            tool,
            old_name: old_name.to_owned(),
            new_name: new_name.to_owned(),
        }
    }

    #[test]
    fn rename_updates_directory_and_config() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(
            &ps,
            &cs,
            "default",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            None,
        )
        .unwrap();

        run_inner(rename_args(Tool::Claude, "default", "work"), tmp.path()).unwrap();

        let config = cs.load().unwrap();
        assert!(!ps.exists(Tool::Claude, "default"));
        assert!(ps.exists(Tool::Claude, "work"));
        assert!(config.profiles.claude.contains_key("work"));
        assert!(!config.profiles.claude.contains_key("default"));
    }

    #[test]
    fn rename_active_profile_updates_active_reference() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(
            &ps,
            &cs,
            "default",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            None,
        )
        .unwrap();
        cs.set_active(Tool::Claude, "default").unwrap();

        run_inner(rename_args(Tool::Claude, "default", "work"), tmp.path()).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(config.active.claude.as_deref(), Some("work"));
    }

    #[test]
    fn rename_rejects_duplicate_target() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(
            &ps,
            &cs,
            "default",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            None,
        )
        .unwrap();
        auth::claude::add_api_key(
            &ps,
            &cs,
            "work",
            "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            None,
        )
        .unwrap();

        let err = run_inner(rename_args(Tool::Claude, "default", "work"), tmp.path()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }
}
