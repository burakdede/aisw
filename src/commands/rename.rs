use std::path::Path;

use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;

use crate::auth;
use crate::cli::RenameArgs;
use crate::config::ConfigStore;
use crate::output;
use crate::profile::{validate_profile_name, ProfileStore};

pub fn run(args: RenameArgs, home: &Path) -> Result<()> {
    let (old_name, new_name) = resolve_names(&args, home)?;
    run_inner(
        RenameArgs {
            old_name: Some(old_name),
            new_name: Some(new_name),
            ..args
        },
        home,
    )
}

pub(crate) fn run_inner(args: RenameArgs, home: &Path) -> Result<()> {
    let old_name = args
        .old_name
        .as_deref()
        .context("rename requires an existing profile name")?;
    let new_name = args
        .new_name
        .as_deref()
        .context("rename requires a new profile name")?;
    validate_profile_name(new_name)?;

    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;

    if old_name == new_name {
        anyhow::bail!("profile '{}' is already named '{}'.", old_name, new_name);
    }

    let profiles = config.profiles_for(args.tool);
    let profile_meta = match profiles.get(old_name) {
        Some(m) => m,
        None => {
            let profile_names: Vec<&str> = profiles.keys().map(String::as_str).collect();
            let suggestion = crate::util::edit_distance::closest_match(old_name, &profile_names, 2);
            if let Some(hint) = suggestion {
                anyhow::bail!(
                    "profile '{}' not found for {}.\n  Did you mean '{}'?",
                    old_name,
                    args.tool,
                    hint
                );
            } else {
                anyhow::bail!("profile '{}' not found for {}.", old_name, args.tool);
            }
        }
    };
    profile_meta
        .credential_backend
        .validate_for_tool(args.tool)?;

    profile_store.rename(args.tool, old_name, new_name)?;

    if profile_meta.credential_backend == crate::config::CredentialBackend::SystemKeyring {
        if let Err(err) = auth::secure_store::rename_profile_secret(args.tool, old_name, new_name) {
            let _ = profile_store.rename(args.tool, new_name, old_name);
            return Err(err).context(format!(
                "rolled back secure credential rename after keychain update failed for {}",
                args.tool
            ));
        }
    }

    if let Err(err) = config_store.rename_profile(args.tool, old_name, new_name) {
        if profile_meta.credential_backend == crate::config::CredentialBackend::SystemKeyring {
            let _ = auth::secure_store::rename_profile_secret(args.tool, new_name, old_name);
        }
        let _ = profile_store.rename(args.tool, new_name, old_name);
        return Err(err).context(format!(
            "rolled back profile directory rename after config update failed for {}",
            args.tool
        ));
    }

    output::print_title("Renamed profile");
    output::print_kv("Tool", args.tool.display_name());
    output::print_kv("Previous", old_name);
    output::print_kv("New", new_name);
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Stored profile renamed.");
    output::print_effect("Config references updated.");
    output::print_blank_line();
    output::print_next_step("Run 'aisw list' to review stored profiles.");
    Ok(())
}

fn resolve_names(args: &RenameArgs, home: &Path) -> Result<(String, String)> {
    let (old_input, new_input) = match (args.old_name.as_deref(), args.new_name.as_deref()) {
        (Some(old), Some(new)) => (Some(old.to_owned()), Some(new.to_owned())),
        (None, Some(new)) => (None, Some(new.to_owned())),
        (Some(new_only), None) => (None, Some(new_only.to_owned())),
        (None, None) => {
            anyhow::bail!(
                "rename requires a new profile name.\n  Re-run as: aisw rename {} <old> <new>",
                args.tool
            )
        }
    };

    let new_name = new_input.expect("validated above");
    validate_profile_name(&new_name)?;

    if let Some(old) = old_input {
        return Ok((old, new_name));
    }

    let config = ConfigStore::new(home).load()?;
    let profiles = config.profiles_for(args.tool);
    if profiles.is_empty() {
        anyhow::bail!(
            "no profiles stored for {}.\n  Add one first with: aisw add {} <profile> --api-key <key>",
            args.tool.display_name(),
            args.tool
        );
    }

    if crate::runtime::is_non_interactive() {
        anyhow::bail!(
            "rename requires an existing profile name in non-interactive mode.\n  Re-run as: aisw rename {} <old> <new>",
            args.tool
        );
    }

    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        anyhow::bail!(
            "rename without an existing profile requires an interactive TTY.\n  Re-run as: aisw rename {} <old> <new>",
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
            "Choose {} profile to rename (Esc/Ctrl-C to cancel)",
            args.tool.display_name()
        ))
        .items(&names)
        .default(default_index)
        .interact()?;

    Ok((names[selection].to_owned(), new_name))
}

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
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

    fn rename_args(tool: Tool, old_name: &str, new_name: &str) -> RenameArgs {
        RenameArgs {
            tool,
            old_name: Some(old_name.to_owned()),
            new_name: Some(new_name.to_owned()),
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
        assert!(config.profiles_for(Tool::Claude).contains_key("work"));
        assert!(!config.profiles_for(Tool::Claude).contains_key("default"));
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
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
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

    #[test]
    fn rename_moves_secure_profile_secret() {
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
        ps.create(Tool::Claude, "default").unwrap();
        auth::secure_store::write_profile_secret(Tool::Claude, "default", br#"{"token":"tok"}"#)
            .unwrap();
        cs.add_profile(
            Tool::Claude,
            "default",
            ProfileMeta {
                added_at: chrono::Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: CredentialBackend::SystemKeyring,
                label: None,
            },
        )
        .unwrap();

        run_inner(rename_args(Tool::Claude, "default", "work"), tmp.path()).unwrap();

        assert!(
            auth::secure_store::read_profile_secret(Tool::Claude, "default")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            auth::secure_store::read_profile_secret(Tool::Claude, "work")
                .unwrap()
                .as_deref(),
            Some(br#"{"token":"tok"}"#.as_slice())
        );
    }
}
