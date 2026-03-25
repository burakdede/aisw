use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;

use crate::auth;
use crate::config::{AuthMethod, ConfigStore, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::Tool;

// Marker written by shell_hook.rs — must match.
const HOOK_MARKER: &str = "# Added by aisw";

pub(crate) fn run_inner(
    aisw_home: &Path,
    user_home: &Path,
    shell_env: Option<&str>,
    confirmed: bool,
) -> Result<()> {
    // Ensure ~/.aisw/ exists with a default config.json.
    fs::create_dir_all(aisw_home)
        .with_context(|| format!("could not create {}", aisw_home.display()))?;
    let config_store = ConfigStore::new(aisw_home);
    config_store.load()?; // creates config.json with defaults if absent
    println!("Created {}.", aisw_home.display());

    // Shell hook installation.
    let shell_name = shell_env
        .and_then(|s| Path::new(s).file_name())
        .and_then(|n| n.to_str());
    match shell_name {
        Some(s @ ("bash" | "zsh" | "fish")) => {
            install_shell_hook(user_home, s, confirmed)?;
        }
        Some(name) => {
            println!(
                "Shell not recognized ({}). Install the hook manually: \
                 aisw shell-hook bash >> ~/.bashrc",
                name
            );
        }
        None => {
            println!(
                "Could not detect shell. Install the hook manually: \
                 aisw shell-hook bash >> ~/.bashrc"
            );
        }
    }

    // Credential import.
    import_credentials(aisw_home, user_home, confirmed)?;

    println!("\nSetup complete. Run 'aisw add <tool> <name>' to add more accounts.");
    Ok(())
}

fn rc_file(user_home: &Path, shell: &str) -> PathBuf {
    match shell {
        "bash" => {
            if cfg!(target_os = "macos") {
                user_home.join(".bash_profile")
            } else {
                user_home.join(".bashrc")
            }
        }
        "zsh" => user_home.join(".zshrc"),
        "fish" => user_home.join(".config").join("fish").join("config.fish"),
        _ => unreachable!(),
    }
}

fn install_shell_hook(user_home: &Path, shell: &str, confirmed: bool) -> Result<()> {
    let rc = rc_file(user_home, shell);

    if rc.exists() {
        let contents =
            fs::read_to_string(&rc).with_context(|| format!("could not read {}", rc.display()))?;
        if contents.contains(HOOK_MARKER) {
            println!("Shell hook already installed in {}.", rc.display());
            return Ok(());
        }
    }

    let should_install = confirmed
        || prompt_yes_no(&format!(
            "Shell: {}\nAdd shell integration to {}? [Y/n] ",
            shell,
            rc.display()
        ));

    if !should_install {
        println!("Skipping shell hook installation.");
        return Ok(());
    }

    if let Some(parent) = rc.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    let hook_line = match shell {
        "bash" | "zsh" => format!("\n{}\neval \"$(aisw shell-hook {})\"\n", HOOK_MARKER, shell),
        "fish" => format!("\n{}\naisw shell-hook fish | source\n", HOOK_MARKER),
        _ => unreachable!(),
    };

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc)
        .with_context(|| format!("could not open {}", rc.display()))?;
    file.write_all(hook_line.as_bytes())
        .with_context(|| format!("could not write to {}", rc.display()))?;

    println!(
        "  Appended to {}. Restart your shell or run: source {}",
        rc.display(),
        rc.display()
    );
    Ok(())
}

fn prompt_yes_no(prompt: &str) -> bool {
    eprint!("{}", prompt);
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap_or(0);
    matches!(line.trim(), "" | "y" | "Y")
}

fn import_credentials(aisw_home: &Path, user_home: &Path, confirmed: bool) -> Result<()> {
    println!("\nImport existing credentials as profiles?");
    import_claude(aisw_home, user_home, confirmed)?;
    import_codex(aisw_home, user_home, confirmed)?;
    import_gemini(aisw_home, user_home, confirmed)?;
    Ok(())
}

fn import_claude(aisw_home: &Path, user_home: &Path, confirmed: bool) -> Result<()> {
    let candidates = [
        user_home.join(".claude").join(".credentials.json"),
        user_home
            .join(".config")
            .join("claude")
            .join(".credentials.json"),
    ];
    let Some(src) = candidates.iter().find(|p| p.exists()) else {
        println!("  Claude Code: no existing credentials found.");
        return Ok(());
    };

    let profile_store = ProfileStore::new(aisw_home);
    let config_store = ConfigStore::new(aisw_home);

    if profile_store.exists(Tool::Claude, "default") {
        println!("  Claude Code: profile 'default' already exists, skipping.");
        return Ok(());
    }

    println!("  Claude Code: found {}", src.display());
    let should_import = confirmed || prompt_yes_no("  Save as profile \"default\"? [Y/n] ");
    if !should_import {
        return Ok(());
    }

    profile_store.create(Tool::Claude, "default")?;
    profile_store.copy_file_into(Tool::Claude, "default", src, ".credentials.json")?;
    config_store.add_profile(
        Tool::Claude,
        "default",
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            label: Some("imported".to_owned()),
        },
    )?;
    println!("  Imported Claude Code credentials as profile 'default'.");
    Ok(())
}

fn import_codex(aisw_home: &Path, user_home: &Path, confirmed: bool) -> Result<()> {
    let src = user_home.join(".codex").join("auth.json");
    if !src.exists() {
        println!("  Codex CLI: no existing credentials found.");
        return Ok(());
    }

    let profile_store = ProfileStore::new(aisw_home);
    let config_store = ConfigStore::new(aisw_home);

    if profile_store.exists(Tool::Codex, "default") {
        println!("  Codex CLI: profile 'default' already exists, skipping.");
        return Ok(());
    }

    println!("  Codex CLI: found {}", src.display());
    let should_import = confirmed || prompt_yes_no("  Save as profile \"default\"? [Y/n] ");
    if !should_import {
        return Ok(());
    }

    profile_store.create(Tool::Codex, "default")?;
    auth::codex::write_file_store_config(&profile_store, "default")?;
    profile_store.copy_file_into(Tool::Codex, "default", &src, "auth.json")?;
    config_store.add_profile(
        Tool::Codex,
        "default",
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            label: Some("imported".to_owned()),
        },
    )?;
    println!("  Imported Codex CLI credentials as profile 'default'.");
    Ok(())
}

fn import_gemini(aisw_home: &Path, user_home: &Path, confirmed: bool) -> Result<()> {
    let gemini_dir = user_home.join(".gemini");
    let env_file = gemini_dir.join(".env");
    let settings_file = gemini_dir.join("settings.json");

    let (src, filename, method) = if env_file.exists() {
        (&env_file, ".env", AuthMethod::ApiKey)
    } else if settings_file.exists() {
        (&settings_file, "settings.json", AuthMethod::OAuth)
    } else {
        println!("  Gemini CLI: no existing credentials found.");
        return Ok(());
    };

    let profile_store = ProfileStore::new(aisw_home);
    let config_store = ConfigStore::new(aisw_home);

    if profile_store.exists(Tool::Gemini, "default") {
        println!("  Gemini CLI: profile 'default' already exists, skipping.");
        return Ok(());
    }

    println!("  Gemini CLI: found {}", src.display());
    let should_import = confirmed || prompt_yes_no("  Save as profile \"default\"? [Y/n] ");
    if !should_import {
        return Ok(());
    }

    profile_store.create(Tool::Gemini, "default")?;
    profile_store.copy_file_into(Tool::Gemini, "default", src, filename)?;
    config_store.add_profile(
        Tool::Gemini,
        "default",
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: method,
            label: Some("imported".to_owned()),
        },
    )?;
    println!("  Imported Gemini CLI credentials as profile 'default'.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn run(aisw_home: &Path, user_home: &Path, shell: Option<&str>) -> Result<()> {
        run_inner(aisw_home, user_home, shell, true)
    }

    #[test]
    fn creates_aisw_home_and_config() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        run(&aisw_home, &user_home, None).unwrap();

        assert!(aisw_home.join("config.json").exists());
    }

    #[test]
    fn idempotent_does_not_duplicate_hook() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();
        let rc = user_home.join(".zshrc");

        run(&aisw_home, &user_home, Some("/bin/zsh")).unwrap();
        run(&aisw_home, &user_home, Some("/bin/zsh")).unwrap();

        let contents = fs::read_to_string(&rc).unwrap();
        assert_eq!(
            contents.matches("shell-hook").count(),
            1,
            "hook should appear exactly once"
        );
    }

    #[test]
    fn bash_rc_file_appended() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        run(&aisw_home, &user_home, Some("/bin/bash")).unwrap();

        let rc = if cfg!(target_os = "macos") {
            user_home.join(".bash_profile")
        } else {
            user_home.join(".bashrc")
        };
        let contents = fs::read_to_string(&rc).unwrap();
        assert!(contents.contains("shell-hook bash"));
    }

    #[test]
    fn fish_rc_file_appended() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        run(&aisw_home, &user_home, Some("/usr/bin/fish")).unwrap();

        let rc = user_home.join(".config").join("fish").join("config.fish");
        let contents = fs::read_to_string(&rc).unwrap();
        assert!(contents.contains("shell-hook fish | source"));
    }

    #[test]
    fn unknown_shell_does_not_error() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        assert!(run(&aisw_home, &user_home, Some("/usr/bin/nushell")).is_ok());
    }

    #[test]
    fn imports_claude_credentials() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        let claude_dir = user_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join(".credentials.json"),
            b"{\"token\":\"oauth\"}",
        )
        .unwrap();

        run(&aisw_home, &user_home, None).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Claude, "default"));
        let contents = ps
            .read_file(Tool::Claude, "default", ".credentials.json")
            .unwrap();
        assert_eq!(contents, b"{\"token\":\"oauth\"}");

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert!(config.profiles.claude.contains_key("default"));
        assert_eq!(
            config.profiles.claude["default"].auth_method,
            AuthMethod::OAuth
        );
    }

    #[test]
    fn imports_codex_credentials() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        let codex_dir = user_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("auth.json"), b"{\"token\":\"tok\"}").unwrap();

        run(&aisw_home, &user_home, None).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Codex, "default"));
        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert_eq!(
            config.profiles.codex["default"].auth_method,
            AuthMethod::OAuth
        );
    }

    #[test]
    fn imports_gemini_env_as_api_key() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        let gemini_dir = user_home.join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();
        fs::write(gemini_dir.join(".env"), b"GEMINI_API_KEY=abc123\n").unwrap();

        run(&aisw_home, &user_home, None).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Gemini, "default"));
        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert_eq!(
            config.profiles.gemini["default"].auth_method,
            AuthMethod::ApiKey
        );
    }

    #[test]
    fn skip_import_if_default_profile_exists() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        let claude_dir = user_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join(".credentials.json"), b"{\"token\":\"v1\"}").unwrap();

        // First run: import succeeds.
        run(&aisw_home, &user_home, None).unwrap();

        // Second run: skip without error.
        run(&aisw_home, &user_home, None).unwrap();

        // Profile still exists, credentials not overwritten.
        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Claude, "default"));
    }
}
