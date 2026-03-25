use std::path::Path;

use anyhow::{Context, Result};

use crate::auth;
use crate::backup::BackupManager;
use crate::cli::UseArgs;
use crate::config::{AuthMethod, ConfigStore};
use crate::next_steps;
use crate::profile::ProfileStore;
use crate::types::Tool;

pub fn run(args: UseArgs, home: &Path) -> Result<()> {
    let user_home = dirs::home_dir().context("could not determine home directory")?;
    run_in(args, home, &user_home)
}

pub(crate) fn run_in(args: UseArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;

    let profiles = match args.tool {
        Tool::Claude => &config.profiles.claude,
        Tool::Codex => &config.profiles.codex,
        Tool::Gemini => &config.profiles.gemini,
    };

    let profile_meta = profiles.get(&args.profile_name).ok_or_else(|| {
        anyhow::anyhow!(
            "profile '{}' not found for {}.\n  \
             Run 'aisw list {}' to see available profiles.",
            args.profile_name,
            args.tool,
            args.tool
        )
    })?;

    if config.settings.backup_on_switch {
        let backup_manager = BackupManager::new(home);
        let profile_dir = profile_store.profile_dir(args.tool, &args.profile_name);
        backup_manager.snapshot(args.tool, &args.profile_name, &profile_dir, profile_meta)?;
    }

    match args.tool {
        Tool::Claude => match profile_meta.auth_method {
            AuthMethod::OAuth => {
                if args.emit_env {
                    let profile_dir = profile_store.profile_dir(Tool::Claude, &args.profile_name);
                    println!("export CLAUDE_CONFIG_DIR={}", profile_dir.display());
                } else {
                    auth::claude::apply_live_credentials(
                        &profile_store,
                        &args.profile_name,
                        user_home,
                    )?;
                }
            }
            AuthMethod::ApiKey => {
                if args.emit_env {
                    let key = auth::claude::read_api_key(&profile_store, &args.profile_name)?;
                    println!("export ANTHROPIC_API_KEY={}", key);
                } else {
                    auth::claude::apply_live_credentials(
                        &profile_store,
                        &args.profile_name,
                        user_home,
                    )?;
                }
            }
        },
        Tool::Codex => match profile_meta.auth_method {
            AuthMethod::OAuth => {
                if args.emit_env {
                    let profile_dir = profile_store.profile_dir(Tool::Codex, &args.profile_name);
                    println!("export CODEX_HOME={}", profile_dir.display());
                } else {
                    auth::codex::apply_live_files(&profile_store, &args.profile_name, user_home)?;
                }
            }
            AuthMethod::ApiKey => {
                if args.emit_env {
                    let key = auth::codex::read_api_key(&profile_store, &args.profile_name)?;
                    println!("export OPENAI_API_KEY={}", key);
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
                    auth::gemini::apply_env_file(
                        &profile_store,
                        &args.profile_name,
                        &gemini_dir.join(".env"),
                    )?;
                }
                AuthMethod::OAuth => {
                    auth::gemini::apply_token_cache(
                        &profile_store,
                        &args.profile_name,
                        &gemini_dir,
                    )?;
                }
            }
        }
    }

    config_store.set_active(args.tool, &args.profile_name)?;

    if !args.emit_env {
        println!("Switched {} to profile '{}'.", args.tool, args.profile_name);
        println!("{}", next_steps::after_use());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::cli::UseArgs;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

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
    fn claude_api_key_emit_env_prints_anthropic_key() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        // run_in with emit_env=true — output goes to stdout (captured by test runner,
        // not easily assertable here; we verify no error and config updated).
        run_in(use_args(Tool::Claude, "work", true), &home, &user_home).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active.claude.as_deref(), Some("work"));
    }

    #[test]
    fn use_updates_active_in_config() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        run_in(use_args(Tool::Claude, "work", false), &home, &user_home).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active.claude.as_deref(), Some("work"));
    }

    #[test]
    fn use_creates_backup_when_enabled() {
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
        assert_eq!(config.active.codex.as_deref(), Some("work"));
    }
}
