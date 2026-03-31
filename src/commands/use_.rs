use std::path::Path;

use anyhow::{Context, Result};

use crate::auth;
use crate::backup::BackupManager;
use crate::cli::UseArgs;
use crate::config::{AuthMethod, ConfigStore};
use crate::output;
use crate::profile::ProfileStore;
use crate::types::{StateMode, Tool};

fn emit_export(name: &str, value: &str) {
    let escaped = value.replace('\'', "'\"'\"'");
    println!("export {}='{}'", name, escaped);
}

pub fn run(args: UseArgs, home: &Path) -> Result<()> {
    let user_home = dirs::home_dir().context("could not determine home directory")?;
    run_in(args, home, &user_home)
}

pub(crate) fn run_in(args: UseArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;
    let requested_state_mode = match (args.tool, args.state_mode) {
        (tool, mode) if tool.supports_state_mode() => mode,
        (_, Some(_)) => {
            anyhow::bail!(
                "--state-mode is currently supported only for claude and codex.\n  \
                 Gemini remains isolated-only because its native ~/.gemini directory mixes \
                 credentials with broader local state such as history, trusted folders, \
                 project mappings, settings, and MCP config."
            );
        }
        (_, None) => None,
    };
    let state_mode = if args.tool.supports_state_mode() {
        requested_state_mode.unwrap_or(config.state_mode_for(args.tool))
    } else {
        StateMode::Isolated
    };

    let profiles = config.profiles_for(args.tool);

    let profile_meta = profiles.get(&args.profile_name).ok_or_else(|| {
        anyhow::anyhow!(
            "profile '{}' not found for {}.\n  \
             Run 'aisw list {}' to see available profiles.",
            args.profile_name,
            args.tool,
            args.tool
        )
    })?;
    profile_meta
        .credential_backend
        .validate_for_tool(args.tool)?;

    if config.settings.backup_on_switch {
        let backup_manager = BackupManager::new(home);
        let profile_dir = profile_store.profile_dir(args.tool, &args.profile_name);
        backup_manager.snapshot(args.tool, &args.profile_name, &profile_dir, profile_meta)?;
    }

    match args.tool {
        Tool::Claude => match profile_meta.auth_method {
            AuthMethod::OAuth => {
                if args.emit_env {
                    auth::claude::emit_shell_env(&args.profile_name, &profile_store, state_mode);
                } else {
                    if cfg!(target_os = "macos") {
                        output::print_info(
                            "Claude on macOS stores live auth in Keychain. Switching this profile may trigger a macOS Keychain prompt so aisw can update Claude's active credentials.",
                        );
                        output::print_blank_line();
                    }
                    auth::claude::apply_live_credentials(
                        &profile_store,
                        &args.profile_name,
                        profile_meta.credential_backend,
                        user_home,
                    )?;
                }
            }
            AuthMethod::ApiKey => {
                if args.emit_env {
                    auth::claude::emit_shell_env(&args.profile_name, &profile_store, state_mode);
                } else {
                    if cfg!(target_os = "macos") {
                        output::print_info(
                            "Claude on macOS stores live auth in Keychain. Switching this profile may trigger a macOS Keychain prompt so aisw can update Claude's active credentials.",
                        );
                        output::print_blank_line();
                    }
                    auth::claude::apply_live_credentials(
                        &profile_store,
                        &args.profile_name,
                        profile_meta.credential_backend,
                        user_home,
                    )?;
                }
            }
        },
        Tool::Codex => match profile_meta.auth_method {
            AuthMethod::OAuth => {
                if args.emit_env {
                    auth::codex::emit_shell_env(&args.profile_name, &profile_store, state_mode);
                } else {
                    auth::codex::apply_live_files(&profile_store, &args.profile_name, user_home)?;
                }
            }
            AuthMethod::ApiKey => {
                if args.emit_env {
                    match state_mode {
                        StateMode::Isolated => auth::codex::emit_shell_env(
                            &args.profile_name,
                            &profile_store,
                            state_mode,
                        ),
                        StateMode::Shared => {
                            println!("unset CODEX_HOME");
                        }
                    }
                } else {
                    auth::codex::apply_live_files(&profile_store, &args.profile_name, user_home)?;
                }
            }
        },
        Tool::Gemini => {
            let gemini_dir = user_home.join(".gemini");
            std::fs::create_dir_all(&gemini_dir)
                .with_context(|| format!("could not create {}", gemini_dir.display()))?;
            match profile_meta.auth_method {
                AuthMethod::ApiKey => {
                    if args.emit_env {
                        let key = auth::gemini::read_api_key(&profile_store, &args.profile_name)?;
                        emit_export("GEMINI_API_KEY", &key);
                    } else {
                        auth::gemini::apply_env_file(
                            &profile_store,
                            &args.profile_name,
                            &gemini_dir.join(".env"),
                        )?;
                    }
                }
                AuthMethod::OAuth => {
                    if args.emit_env {
                        println!("unset GEMINI_API_KEY");
                    } else {
                        auth::gemini::apply_token_cache(
                            &profile_store,
                            &args.profile_name,
                            &gemini_dir,
                        )?;
                    }
                }
            }
        }
    }

    config_store.activate_profile(
        args.tool,
        &args.profile_name,
        args.tool.supports_state_mode().then_some(state_mode),
    )?;

    if !args.emit_env {
        output::print_title("Switched profile");
        output::print_kv("Tool", args.tool.display_name());
        output::print_kv("Active profile", &args.profile_name);
        output::print_kv("Auth", auth_label(profile_meta.auth_method));
        output::print_kv("Backend", profile_meta.credential_backend.display_name());
        if args.tool.supports_state_mode() {
            output::print_kv("State mode", state_mode.display_name());
        }
        output::print_blank_line();
        output::print_effects_header();
        output::print_effect("Live tool configuration updated.");
        output::print_effect("Active profile updated.");
        if args.tool.supports_state_mode() {
            output::print_effect(match (args.tool, state_mode) {
                (Tool::Claude, StateMode::Isolated) => {
                    "Claude will use isolated profile state when shell integration is active."
                }
                (Tool::Claude, StateMode::Shared) => {
                    "Claude will keep shared local state and only switch account credentials."
                }
                (Tool::Codex, StateMode::Isolated) => {
                    "Codex will use isolated profile state when shell integration is active."
                }
                (Tool::Codex, StateMode::Shared) => {
                    "Codex will keep shared local state and only switch account credentials."
                }
                (Tool::Gemini, _) => unreachable!(),
            });
        }
        if config.settings.backup_on_switch {
            output::print_effect("Backup created before switching.");
        }
        output::print_blank_line();
        output::print_next_step(output::next_step_after_use());
    }

    Ok(())
}

fn auth_label(method: AuthMethod) -> &'static str {
    match method {
        AuthMethod::OAuth => "oauth",
        AuthMethod::ApiKey => "api_key",
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::cli::UseArgs;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
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
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    fn claude_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn setup_claude_api_key_profile(home: &Path, name: &str) {
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        auth::claude::add_api_key(&ps, &cs, name, claude_key(), None).unwrap();
    }

    fn setup_gemini_api_key_profile(home: &Path, name: &str) {
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        auth::gemini::add_api_key(&ps, &cs, name, "AIzatest1234567890ABCDEF", None).unwrap();
    }

    fn use_args(tool: Tool, name: &str, emit_env: bool) -> UseArgs {
        UseArgs {
            tool,
            profile_name: name.to_owned(),
            state_mode: None,
            emit_env,
        }
    }

    #[test]
    fn nonexistent_profile_errors() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();

        let err = run_in(use_args(Tool::Claude, "ghost", false), &home, &user_home).unwrap_err();
        assert!(err.to_string().contains("not found"), "unexpected: {}", err);
    }

    #[test]
    fn claude_api_key_emit_env_updates_active() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        // run_in with emit_env=true — output goes to stdout (captured by test runner,
        // not easily assertable here; we verify no error and config updated).
        run_in(use_args(Tool::Claude, "work", true), &home, &user_home).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
    }

    #[test]
    fn use_updates_active_in_config() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        run_in(use_args(Tool::Claude, "work", false), &home, &user_home).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
    }

    #[test]
    fn use_creates_backup_when_enabled() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        run_in(use_args(Tool::Claude, "work", false), &home, &user_home).unwrap();

        let backups_dir = home.join("backups");
        assert!(backups_dir.exists(), "backups dir should be created");
        let entries: Vec<_> = fs::read_dir(&backups_dir).unwrap().collect();
        assert!(!entries.is_empty(), "at least one backup entry expected");
    }

    #[test]
    fn gemini_api_key_writes_env_file() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        setup_gemini_api_key_profile(&home, "work");

        run_in(use_args(Tool::Gemini, "work", false), &home, &user_home).unwrap();

        let env_file = user_home.join(".gemini").join(".env");
        assert!(env_file.exists(), ".env should be written to gemini dir");
        let contents = fs::read_to_string(&env_file).unwrap();
        assert!(contents.contains("GEMINI_API_KEY="));
    }

    #[test]
    fn codex_api_key_emit_env_updates_active() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        let ps = ProfileStore::new(&home);
        let cs = ConfigStore::new(&home);
        auth::codex::add_api_key(&ps, &cs, "work", "sk-codex-test-key-12345", None).unwrap();

        run_in(use_args(Tool::Codex, "work", true), &home, &user_home).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(config.active_for(Tool::Codex), Some("work"));
    }
}
