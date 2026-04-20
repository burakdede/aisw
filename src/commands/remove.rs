use std::path::Path;

use anyhow::{bail, Context, Result};
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;

use crate::auth;
use crate::backup::BackupManager;
use crate::cli::RemoveArgs;
use crate::config::{Config, ConfigStore};
use crate::output;
use crate::profile::ProfileStore;
use crate::runtime;
use crate::types::Tool;

pub fn run(args: RemoveArgs, home: &Path) -> Result<()> {
    let profile_name = resolve_profile_name(&args, home)?;
    let args = RemoveArgs {
        profile_name: Some(profile_name),
        ..args
    };

    if !args.yes {
        if runtime::is_non_interactive() {
            bail!(
                "remove requires confirmation.\n  \
                 Re-run with --yes, or omit --non-interactive."
            );
        }
        // Validate before prompting — better UX to fail fast on invalid ops.
        precheck(&args, home)?;
        let profile_name = args
            .profile_name
            .as_deref()
            .context("remove requires a profile name")?;
        eprint!(
            "Remove {} profile '{}'? This cannot be undone. [y/N] ",
            args.tool, profile_name
        );
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .context("could not read confirmation from stdin")?;
        if !matches!(line.trim(), "y" | "Y") {
            bail!("operation cancelled by user.");
        }
    }
    run_inner(args, home, true)
}

/// Entry point for non-interactive use (tests and `--yes` flag).
pub(crate) fn run_inner(args: RemoveArgs, home: &Path, confirmed: bool) -> Result<()> {
    let profile_name = args
        .profile_name
        .as_deref()
        .context("remove requires a profile name")?;
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;

    if !profile_store.exists(args.tool, profile_name) {
        let profile_names: Vec<&str> = config
            .profiles_for(args.tool)
            .keys()
            .map(String::as_str)
            .collect();
        let suggestion = crate::util::edit_distance::closest_match(profile_name, &profile_names, 2);
        if let Some(hint) = suggestion {
            bail!(
                "profile '{}' not found for {}.\n  Did you mean '{}'?\n  \
                 Run 'aisw list {}' to see available profiles.",
                profile_name,
                args.tool,
                hint,
                args.tool
            );
        } else {
            bail!(
                "profile '{}' not found for {}.\n  \
                 Run 'aisw list {}' to see available profiles.",
                profile_name,
                args.tool,
                args.tool
            );
        }
    }

    let is_active = active_for(&config, args.tool) == Some(profile_name);
    if is_active && !args.force {
        bail!(
            "profile '{}' is currently active. \
             Switch to another profile first, or use --force.",
            profile_name
        );
    }

    if !confirmed {
        bail!("operation cancelled by user.");
    }

    // Final backup before deleting.
    let profile_dir = profile_store.profile_dir(args.tool, profile_name);
    let profile_meta = config
        .profiles_for(args.tool)
        .get(profile_name)
        .with_context(|| {
            format!(
                "profile '{}' exists on disk for {} but is missing from config",
                profile_name, args.tool
            )
        })?;
    profile_meta
        .credential_backend
        .validate_for_tool(args.tool)?;
    BackupManager::new(home).snapshot(args.tool, profile_name, &profile_dir, profile_meta)?;

    if profile_meta.credential_backend == crate::config::CredentialBackend::SystemKeyring {
        auth::secure_store::delete_profile_secret(args.tool, profile_name)?;
    }
    profile_store.delete(args.tool, profile_name)?;
    config_store.remove_profile(args.tool, profile_name)?;

    if is_active {
        config_store.clear_active(args.tool)?;
    }

    output::print_title("Removed profile");
    output::print_kv("Tool", args.tool.display_name());
    output::print_kv("Profile", profile_name);
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
    let profile_name = args
        .profile_name
        .as_deref()
        .context("remove requires a profile name")?;
    let profile_store = ProfileStore::new(home);
    if !profile_store.exists(args.tool, profile_name) {
        bail!(
            "profile '{}' not found for {}.\n  \
             Run 'aisw list {}' to see available profiles.",
            profile_name,
            args.tool,
            args.tool
        );
    }
    let config = ConfigStore::new(home).load()?;
    let is_active = active_for(&config, args.tool) == Some(profile_name);
    if is_active && !args.force {
        bail!(
            "profile '{}' is currently active. \
             Switch to another profile first, or use --force.",
            profile_name
        );
    }
    Ok(())
}

fn resolve_profile_name(args: &RemoveArgs, home: &Path) -> Result<String> {
    if let Some(name) = args.profile_name.as_deref() {
        return Ok(name.to_owned());
    }

    let config = ConfigStore::new(home).load()?;
    let profiles = config.profiles_for(args.tool);
    if profiles.is_empty() {
        bail!(
            "no profiles stored for {}.\n  Add one first with: aisw add {} <profile> --api-key <key>",
            args.tool.display_name(),
            args.tool
        );
    }

    if runtime::is_non_interactive() {
        bail!(
            "remove requires a profile name in non-interactive mode.\n  Re-run as: aisw remove {} <profile>",
            args.tool
        );
    }

    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!(
            "remove without a profile requires an interactive TTY.\n  Re-run as: aisw remove {} <profile>",
            args.tool
        );
    }

    let mut names: Vec<&str> = profiles.keys().map(String::as_str).collect();
    names.sort_unstable();
    let default_index = config
        .active_for(args.tool)
        .and_then(|active| names.iter().position(|n| *n == active))
        .unwrap_or(0);

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Choose {} profile to remove (Esc/Ctrl-C to cancel)",
            args.tool.display_name()
        ))
        .items(&names)
        .default(default_index)
        .interact()?;

    Ok(names[selection].to_owned())
}

fn active_for(config: &Config, tool: Tool) -> Option<&str> {
    config.active_for(tool)
}

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::cli::RemoveArgs;
    use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
    use crate::profile::ProfileStore;
    use crate::types::Tool;

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

    fn write_security_mock(bin: &std::path::Path) {
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
        fs::set_permissions(bin, fs::Permissions::from_mode(0o755)).unwrap();
    }

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
            profile_name: Some(name.to_owned()),
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
        assert!(!config.profiles_for(Tool::Claude).contains_key("work"));
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
        assert!(!config.profiles_for(Tool::Claude).contains_key("work"));
        assert_eq!(config.active_for(Tool::Claude), None);
    }

    #[test]
    fn unconfirmed_aborts_without_deleting() {
        let tmp = tempdir().unwrap();
        add_claude(tmp.path(), "work");

        let err = run_inner(
            remove_args(Tool::Claude, "work", false, false),
            tmp.path(),
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("cancelled"));

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

    #[test]
    fn remove_deletes_secure_profile_secret() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let security_bin = bin_dir.join("security");
        write_security_mock(&security_bin);
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", tmp.path().join("keychain"));
        let _security = EnvVarGuard::set(
            "AISW_SECURITY_BIN",
            security_bin
                .to_str()
                .expect("security path should be utf-8"),
        );

        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        ps.create(Tool::Claude, "work").unwrap();
        auth::secure_store::write_profile_secret(Tool::Claude, "work", br#"{"token":"tok"}"#)
            .unwrap();
        cs.add_profile(
            Tool::Claude,
            "work",
            ProfileMeta {
                added_at: chrono::Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: CredentialBackend::SystemKeyring,
                label: None,
            },
        )
        .unwrap();

        run_inner(
            remove_args(Tool::Claude, "work", true, false),
            tmp.path(),
            true,
        )
        .unwrap();

        assert!(
            auth::secure_store::read_profile_secret(Tool::Claude, "work")
                .unwrap()
                .is_none()
        );
    }
}
