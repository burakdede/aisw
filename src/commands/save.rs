use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::auth;
use crate::auth::identity;
use crate::cli::SaveArgs;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::output;
use crate::profile::ProfileStore;
use crate::types::Tool;

pub fn run(args: SaveArgs, home: &Path) -> Result<()> {
    let user_home = dirs::home_dir().context("could not determine home directory")?;
    run_in(args, home, &user_home)
}

pub(crate) fn run_in(args: SaveArgs, home: &Path, user_home: &Path) -> Result<()> {
    if args.tool != Tool::Claude {
        bail!(
            "'aisw save' currently only supports claude.\n  \
             For other tools, use 'aisw add {} {}' with --api-key.",
            args.tool,
            args.profile_name,
        );
    }

    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    let snapshot = auth::claude::live_credentials_snapshot_for_import(user_home)?
        .with_context(|| {
            format!(
                "no live credentials found — run 'claude login' first, then retry 'aisw save claude {}'.",
                args.profile_name,
            )
        })?;

    let stored_backend = auth::claude::preferred_import_backend(&snapshot.source);

    profile_store.create(Tool::Claude, &args.profile_name)?;

    let write_result = match stored_backend {
        CredentialBackend::File => profile_store.write_file(
            Tool::Claude,
            &args.profile_name,
            ".credentials.json",
            &snapshot.bytes,
        ),
        CredentialBackend::SystemKeyring => crate::auth::secure_store::write_profile_secret(
            Tool::Claude,
            &args.profile_name,
            &snapshot.bytes,
        ),
    };

    if let Err(e) = write_result {
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        return Err(e);
    }

    if let Err(e) = auth::claude::capture_live_oauth_account_metadata(
        &profile_store,
        &args.profile_name,
        user_home,
    ) {
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        return Err(e);
    }

    if let Err(e) = identity::ensure_unique_oauth_identity(
        &profile_store,
        &config_store,
        Tool::Claude,
        &args.profile_name,
        stored_backend,
    ) {
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        if stored_backend == CredentialBackend::SystemKeyring {
            let _ =
                crate::auth::secure_store::delete_profile_secret(Tool::Claude, &args.profile_name);
        }
        return Err(e);
    }

    if let Err(e) = config_store.add_profile(
        Tool::Claude,
        &args.profile_name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            credential_backend: stored_backend,
            label: args.label.clone(),
        },
    ) {
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        if stored_backend == CredentialBackend::SystemKeyring {
            let _ =
                crate::auth::secure_store::delete_profile_secret(Tool::Claude, &args.profile_name);
        }
        return Err(e);
    }

    if args.set_active {
        config_store.set_active(Tool::Claude, &args.profile_name)?;
    }

    output::print_title("Saved profile");
    output::print_kv("Tool", Tool::Claude.display_name());
    output::print_kv("Profile", &args.profile_name);
    output::print_kv("Backend", stored_backend.display_name());
    output::print_kv(
        "Activation",
        if args.set_active { "active" } else { "stored" },
    );
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Profile credentials stored in aisw.");
    if args.set_active {
        output::print_effect("Active profile updated.");
    }
    output::print_blank_line();
    output::print_next_step(output::next_step_after_add(
        Tool::Claude,
        &args.profile_name,
        args.set_active,
    ));

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::cli::SaveArgs;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    fn write_live_credentials(user_home: &Path, token: &str) {
        let claude_dir = user_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let creds = claude_dir.join(".credentials.json");
        let content =
            format!(r#"{{"oauthToken":"{token}","account":{{"email":"test@example.com"}}}}"#);
        fs::write(&creds, content).unwrap();
        fs::set_permissions(&creds, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn save_args(name: &str, set_active: bool) -> SaveArgs {
        SaveArgs {
            tool: Tool::Claude,
            profile_name: name.to_owned(),
            label: None,
            set_active,
        }
    }

    #[test]
    fn save_creates_profile_from_live_credentials() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        write_live_credentials(&user_home, "live-token-abc");

        run_in(save_args("work", false), &aisw_home, &user_home).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Claude, "work"));

        let stored = ps
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(json["oauthToken"], "live-token-abc");
    }

    #[test]
    fn save_registers_profile_in_config() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        write_live_credentials(&user_home, "live-token-xyz");

        run_in(save_args("personal", false), &aisw_home, &user_home).unwrap();

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert!(config.profiles_for(Tool::Claude).contains_key("personal"));
        assert_eq!(config.active_for(Tool::Claude), None);
    }

    #[test]
    fn save_with_set_active_marks_profile_active() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        write_live_credentials(&user_home, "live-token-set-active");

        run_in(save_args("work-active", true), &aisw_home, &user_home).unwrap();

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work-active"));
    }

    #[test]
    fn save_fails_without_live_credentials() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        // no credentials written

        let err = run_in(save_args("work", false), &aisw_home, &user_home).unwrap_err();
        assert!(
            err.to_string().contains("no live credentials"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn save_rejects_non_claude_tools() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let args = SaveArgs {
            tool: Tool::Codex,
            profile_name: "work".to_owned(),
            label: None,
            set_active: false,
        };
        let err = run_in(args, &aisw_home, &user_home).unwrap_err();
        assert!(
            err.to_string().contains("only supports claude"),
            "unexpected: {err}"
        );
    }
}
