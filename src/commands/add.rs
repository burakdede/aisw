use std::ffi::OsString;
use std::path::Path;

use anyhow::Result;

use crate::auth;
use crate::cli::AddArgs;
use crate::config::ConfigStore;
use crate::next_steps;
use crate::output;
use crate::profile::ProfileStore;
use crate::runtime;
use crate::tool_detection;
use crate::types::Tool;

pub fn run(args: AddArgs, home: &Path) -> Result<()> {
    run_in(args, home, std::env::var_os("PATH").unwrap_or_default())
}

pub(crate) fn run_in(args: AddArgs, home: &Path, tool_path: OsString) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    // Guard: tool binary must be on PATH before we create any profile state.
    let detected = tool_detection::require_in(args.tool, tool_path)?;

    if let Some(ref api_key) = args.api_key {
        match args.tool {
            Tool::Claude => auth::claude::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
            )?,
            Tool::Codex => auth::codex::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
            )?,
            Tool::Gemini => auth::gemini::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
            )?,
        }
    } else {
        if runtime::is_non_interactive() {
            anyhow::bail!(
                "{} requires interactive authentication when --api-key is not provided.\n  \
                 Re-run without --non-interactive, or pass --api-key.",
                args.tool.display_name()
            );
        }
        match args.tool {
            Tool::Claude => auth::claude::add_oauth(
                &profile_store,
                &config_store,
                &args.profile_name,
                args.label.clone(),
                &detected.binary_path,
            )?,
            Tool::Codex => auth::codex::add_oauth(
                &profile_store,
                &config_store,
                &args.profile_name,
                args.label.clone(),
                &detected.binary_path,
            )?,
            Tool::Gemini => auth::gemini::add_oauth(
                &profile_store,
                &config_store,
                &args.profile_name,
                args.label.clone(),
                &detected.binary_path,
            )?,
        }
    }

    if args.set_active {
        config_store.set_active(args.tool, &args.profile_name)?;
    }

    output::print_title("Added profile");
    output::print_kv("Tool", args.tool.display_name());
    output::print_kv("Profile", &args.profile_name);
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
    output::print_next_step(next_steps::after_add(
        args.tool,
        &args.profile_name,
        args.set_active,
    ));

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;
    use crate::types::Tool;

    fn make_fake_binary(dir: &Path, name: &str) {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\necho 'fake 1.0'\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn path_of(dir: &Path) -> OsString {
        dir.as_os_str().to_owned()
    }

    fn claude_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn add_args_api_key(tool: Tool, name: &str, api_key: &str) -> AddArgs {
        AddArgs {
            tool,
            profile_name: name.to_owned(),
            api_key: Some(api_key.to_owned()),
            label: None,
            set_active: false,
        }
    }

    #[test]
    fn tool_not_found_errors() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let args = add_args_api_key(Tool::Claude, "work", claude_key());
        let err = run_in(args, &home, path_of(&bin_dir)).unwrap_err();
        assert!(
            err.to_string().contains("not installed") || err.to_string().contains("not found"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn api_key_claude_creates_profile() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let args = add_args_api_key(Tool::Claude, "work", claude_key());
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert!(config.profiles_for(Tool::Claude).contains_key("work"));
    }

    #[test]
    fn set_active_marks_profile_active() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let mut args = add_args_api_key(Tool::Claude, "work", claude_key());
        args.set_active = true;
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
    }

    #[test]
    fn label_stored_in_config() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let mut args = add_args_api_key(Tool::Claude, "work", claude_key());
        args.label = Some("My work account".to_owned());
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].label.as_deref(),
            Some("My work account")
        );
    }

    #[test]
    fn invalid_profile_name_errors() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let args = add_args_api_key(Tool::Claude, "my profile", claude_key());
        assert!(run_in(args, &home, path_of(&bin_dir)).is_err());
    }

    #[test]
    fn codex_api_key_creates_profile() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "codex");

        let args = add_args_api_key(Tool::Codex, "work", "sk-codex-test-key-12345");
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert!(config.profiles_for(Tool::Codex).contains_key("work"));
    }

    #[test]
    fn gemini_api_key_creates_profile() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "gemini");

        let args = add_args_api_key(Tool::Gemini, "work", "AIzatest1234567890ABCDEF");
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert!(config.profiles_for(Tool::Gemini).contains_key("work"));
    }
}
