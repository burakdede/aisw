use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;

use super::identity;
use crate::config::{AuthMethod, ConfigStore, ProfileMeta};
use crate::live_apply::LiveFileChange;
use crate::profile::ProfileStore;
use crate::types::{StateMode, Tool};

const AUTH_FILE: &str = "auth.json";
const CONFIG_FILE: &str = "config.toml";

// Codex reads credentials from a file rather than the OS keyring when this is set.
const CONFIG_TOML_CONTENTS: &str = "cli_auth_credentials_store = \"file\"\n";

const OAUTH_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

fn live_dir(user_home: &Path) -> PathBuf {
    user_home.join(".codex")
}

fn live_auth_path(user_home: &Path) -> PathBuf {
    live_dir(user_home).join(AUTH_FILE)
}

fn live_config_path(user_home: &Path) -> PathBuf {
    live_dir(user_home).join(CONFIG_FILE)
}

pub(crate) fn write_file_store_config(profile_store: &ProfileStore, name: &str) -> Result<()> {
    profile_store.write_file(
        Tool::Codex,
        name,
        CONFIG_FILE,
        CONFIG_TOML_CONTENTS.as_bytes(),
    )
}

pub fn add_api_key(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    key: &str,
    label: Option<String>,
) -> Result<()> {
    validate_api_key(key)?;

    if let Some(existing_name) = identity::existing_api_key_profile_for_secret(
        profile_store,
        config_store,
        Tool::Codex,
        key,
    )? {
        bail!(
            "Codex API key already exists as profile '{}'.\n  \
             Use that profile or provide a different API key.",
            existing_name
        );
    }

    profile_store.create(Tool::Codex, name)?;

    let cleanup = |ps: &ProfileStore| {
        let _ = ps.delete(Tool::Codex, name);
    };

    write_file_store_config(profile_store, name).inspect_err(|_| cleanup(profile_store))?;

    let auth_json = format!("{{\"token\":\"{}\"}}", key);
    profile_store
        .write_file(Tool::Codex, name, AUTH_FILE, auth_json.as_bytes())
        .inspect_err(|_| cleanup(profile_store))?;

    config_store.add_profile(
        Tool::Codex,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::ApiKey,
            label,
        },
    )?;

    Ok(())
}

pub fn validate_api_key(key: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!(
            "Codex API key must not be empty.\n  \
             Get your API key at platform.openai.com → API Keys."
        );
    }
    Ok(())
}

/// Start the Codex OAuth flow using the installed `codex` binary.
///
/// Pre-writes `config.toml` with `cli_auth_credentials_store = "file"` before
/// spawning so Codex writes `auth.json` into `CODEX_HOME` instead of the OS keyring.
pub fn add_oauth(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    codex_bin: &Path,
) -> Result<()> {
    add_oauth_with(
        profile_store,
        config_store,
        name,
        label,
        codex_bin,
        OAUTH_TIMEOUT,
        POLL_INTERVAL,
    )
}

fn add_oauth_with(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    codex_bin: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<()> {
    let profile_dir = profile_store.create(Tool::Codex, name)?;

    // config.toml must be written before spawning — without it Codex falls back to keyring.
    write_file_store_config(profile_store, name).inspect_err(|_| {
        let _ = profile_store.delete(Tool::Codex, name);
    })?;

    let auth_path =
        run_oauth_flow(codex_bin, &profile_dir, timeout, poll_interval).inspect_err(|_| {
            let _ = profile_store.delete(Tool::Codex, name);
        })?;

    set_auth_permissions(&auth_path)?;
    identity::ensure_unique_oauth_identity(profile_store, config_store, Tool::Codex, name)
        .inspect_err(|_| {
            let _ = profile_store.delete(Tool::Codex, name);
        })?;

    config_store.add_profile(
        Tool::Codex,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            label,
        },
    )?;

    Ok(())
}

fn run_oauth_flow(
    codex_bin: &Path,
    profile_dir: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<PathBuf> {
    let mut child = Command::new(codex_bin)
        .arg("login")
        .env("CODEX_HOME", profile_dir)
        .spawn()
        .with_context(|| format!("could not spawn {}", codex_bin.display()))?;

    let auth_path = profile_dir.join(AUTH_FILE);
    let deadline = Instant::now() + timeout;

    loop {
        if auth_path.exists() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(auth_path);
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "Codex login timed out after {}s. \
                 If auth.json was not written, verify that config.toml has \
                 cli_auth_credentials_store = \"file\" (not \"keyring\").",
                timeout.as_secs()
            );
        }

        std::thread::sleep(poll_interval);
    }
}

#[cfg(unix)]
fn set_auth_permissions(path: &Path) -> Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_auth_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// Read the stored API token from a profile's auth file.
pub fn read_api_key(profile_store: &ProfileStore, name: &str) -> Result<String> {
    let bytes = profile_store.read_file(Tool::Codex, name, AUTH_FILE)?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        anyhow::anyhow!(
            "could not parse auth file for profile '{}'.\n  \
             The profile may be corrupted. Run 'aisw remove codex {}' \
             then 'aisw add codex {}' to reconfigure.\n  \
             ({})",
            name,
            name,
            name,
            e
        )
    })?;
    json["token"].as_str().map(|s| s.to_owned()).ok_or_else(|| {
        anyhow::anyhow!(
            "auth file for profile '{}' is missing the 'token' field.\n  \
                 Run 'aisw remove codex {}' then 'aisw add codex {}' to reconfigure.",
            name,
            name,
            name
        )
    })
}

pub fn apply_live_files(profile_store: &ProfileStore, name: &str, user_home: &Path) -> Result<()> {
    let live_dir = live_dir(user_home);
    std::fs::create_dir_all(&live_dir)
        .with_context(|| format!("could not create {}", live_dir.display()))?;

    let auth_bytes = profile_store.read_file(Tool::Codex, name, AUTH_FILE)?;
    let auth_dest = live_auth_path(user_home);
    let config_dest = live_config_path(user_home);
    let config_bytes = desired_live_file_store_config(user_home)?.into_bytes();

    crate::live_apply::apply_transaction(vec![
        LiveFileChange::write(auth_dest, auth_bytes),
        LiveFileChange::write(config_dest, config_bytes),
    ])
}

pub fn emit_shell_env(name: &str, profile_store: &ProfileStore, mode: StateMode) {
    match mode {
        StateMode::Isolated => {
            let profile_dir = profile_store.profile_dir(Tool::Codex, name);
            println!(
                "export CODEX_HOME={}",
                shell_single_quote(&profile_dir.display().to_string())
            );
        }
        StateMode::Shared => {
            println!("unset CODEX_HOME");
        }
    }
}

pub fn live_files_match(
    profile_store: &ProfileStore,
    name: &str,
    user_home: &Path,
) -> Result<bool> {
    let auth_dest = live_auth_path(user_home);
    if !auth_dest.exists() {
        return Ok(false);
    }
    let live_auth = std::fs::read(&auth_dest)
        .with_context(|| format!("could not read {}", auth_dest.display()))?;
    let stored_auth = profile_store.read_file(Tool::Codex, name, AUTH_FILE)?;
    if live_auth != stored_auth {
        return Ok(false);
    }

    let config_dest = live_config_path(user_home);
    if !config_dest.exists() {
        return Ok(false);
    }
    let config = std::fs::read_to_string(&config_dest)
        .with_context(|| format!("could not read {}", config_dest.display()))?;
    Ok(config_uses_file_store(&config))
}

fn desired_live_file_store_config(user_home: &Path) -> Result<String> {
    let config_dest = live_config_path(user_home);
    if config_dest.exists() {
        let current = std::fs::read_to_string(&config_dest)
            .with_context(|| format!("could not read {}", config_dest.display()))?;
        Ok(merge_file_store_config(&current))
    } else {
        Ok(CONFIG_TOML_CONTENTS.to_owned())
    }
}

fn merge_file_store_config(current: &str) -> String {
    let mut replaced = false;
    let mut lines = Vec::new();
    for line in current.lines() {
        if line.trim_start().starts_with("cli_auth_credentials_store") {
            lines.push(CONFIG_TOML_CONTENTS.trim_end().to_owned());
            replaced = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !replaced {
        if !current.is_empty() && !current.ends_with('\n') {
            lines.push(String::new());
        }
        lines.push(CONFIG_TOML_CONTENTS.trim_end().to_owned());
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn config_uses_file_store(contents: &str) -> bool {
    contents
        .lines()
        .any(|line| line.trim() == "cli_auth_credentials_store = \"file\"")
}

fn shell_single_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;

    fn valid_key() -> &'static str {
        "sk-codex-test-key-12345"
    }

    fn stores(dir: &std::path::Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
    }

    #[test]
    fn validate_accepts_nonempty_key() {
        assert!(validate_api_key(valid_key()).is_ok());
    }

    #[test]
    fn validate_rejects_empty_key() {
        assert!(validate_api_key("").is_err());
        assert!(validate_api_key("   ").is_err());
    }

    #[test]
    fn validate_empty_key_error_mentions_platform() {
        let err = validate_api_key("").unwrap_err();
        assert!(err.to_string().contains("platform.openai.com"));
    }

    #[test]
    fn add_creates_both_files() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());

        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();

        assert!(ps.profile_dir(Tool::Codex, "main").join(AUTH_FILE).exists());
        assert!(ps
            .profile_dir(Tool::Codex, "main")
            .join(CONFIG_FILE)
            .exists());
    }

    #[test]
    fn config_toml_sets_file_store() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();

        let contents = ps.read_file(Tool::Codex, "main", CONFIG_FILE).unwrap();
        let toml_str = std::str::from_utf8(&contents).unwrap();
        assert!(toml_str.contains("cli_auth_credentials_store"));
        assert!(toml_str.contains("file"));
    }

    #[test]
    fn apply_live_files_preserves_existing_config_settings() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();
        std::fs::write(
            user_home.join(".codex").join(CONFIG_FILE),
            "model = \"gpt-5.4\"\n[features]\nunified_exec = true\n",
        )
        .unwrap();

        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();

        apply_live_files(&ps, "main", &user_home).unwrap();

        let contents = std::fs::read_to_string(user_home.join(".codex").join(CONFIG_FILE)).unwrap();
        assert!(contents.contains("model = \"gpt-5.4\""));
        assert!(contents.contains("[features]"));
        assert!(contents.contains("unified_exec = true"));
        assert!(contents.contains("cli_auth_credentials_store = \"file\""));
    }

    #[test]
    fn read_api_key_roundtrip() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();

        assert_eq!(read_api_key(&ps, "main").unwrap(), valid_key());
    }

    #[test]
    fn add_registers_in_config_as_api_key() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), Some("Work".into())).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Codex)["main"].auth_method,
            AuthMethod::ApiKey
        );
        assert_eq!(
            config.profiles_for(Tool::Codex)["main"].label.as_deref(),
            Some("Work")
        );
    }

    #[test]
    #[cfg(unix)]
    fn files_have_600_permissions() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();

        for file in [AUTH_FILE, CONFIG_FILE] {
            let mode = fs::metadata(ps.profile_dir(Tool::Codex, "main").join(file))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{} should be 0600", file);
        }
    }

    #[test]
    fn duplicate_profile_errors() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();
        let err = add_api_key(&ps, &cs, "main", valid_key(), None).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn invalid_key_does_not_create_profile() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", "", None).unwrap_err();
        assert!(!ps.exists(Tool::Codex, "main"));
    }

    // ---- OAuth tests ----

    // Poll interval used in all OAuth tests.
    const TEST_POLL: Duration = Duration::from_millis(10);

    /// Creates a mock binary that either writes auth.json immediately or exits
    /// without writing anything (for timeout tests).
    ///
    /// No `sleep` is used — `sleep` spawns a child process that becomes an orphan
    /// when the parent shell is SIGKILL'd, which can cause ETXTBSY on path reuse.
    #[cfg(unix)]
    fn make_oauth_mock(dir: &std::path::Path, write_auth: bool) -> PathBuf {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let bin = dir.join("codex");
        let body = if write_auth {
            "echo '{\"token\":\"tok\"}' > \"$CODEX_HOME/auth.json\"\n"
        } else {
            "exit 0\n" // exits without writing auth; poll loop times out naturally
        };
        fs::write(&bin, format!("#!/bin/sh\n{}", body)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    #[cfg(unix)]
    fn oauth_config_toml_written_before_spawn() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // Verify config.toml exists in the profile dir when the mock binary runs.
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("codex");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             [ -f \"$CODEX_HOME/config.toml\" ] && touch \"$CODEX_HOME/config_was_present\"\n\
             echo '{}' > \"$CODEX_HOME/auth.json\"\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "main",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let sentinel = ps
            .profile_dir(Tool::Codex, "main")
            .join("config_was_present");
        assert!(
            sentinel.exists(),
            "config.toml was not present when codex was spawned"
        );
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_succeeds() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "main",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(ps.exists(Tool::Codex, "main"));
        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Codex)["main"].auth_method,
            AuthMethod::OAuth
        );
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_times_out_and_cleans_up() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        // Mock exits immediately without writing auth.json.  The poll loop checks
        // for the file (not whether the child is alive), so it keeps retrying until
        // the deadline — no long-lived orphan processes.
        let bin = make_oauth_mock(&bin_dir, false);

        let (ps, cs) = stores(dir.path());
        let err = add_oauth_with(
            &ps,
            &cs,
            "main",
            None,
            &bin,
            Duration::from_millis(200),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(err.to_string().contains("timed out"));
        assert!(!ps.exists(Tool::Codex, "main"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_auth_json_has_600_permissions() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "main",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let path = ps.profile_dir(Tool::Codex, "main").join(AUTH_FILE);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    #[cfg(unix)]
    fn oauth_duplicate_identity_is_rejected_and_cleaned_up() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("codex");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             tmp=\"$CODEX_HOME/auth.json.tmp\"\n\
             echo '{\"account\":{\"email\":\"burak@example.com\"}}' > \"$tmp\"\n\
             mv \"$tmp\" \"$CODEX_HOME/auth.json\"\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        ps.create(Tool::Codex, "work").unwrap();
        write_file_store_config(&ps, "work").unwrap();
        ps.write_file(
            Tool::Codex,
            "work",
            AUTH_FILE,
            br#"{"account":{"email":"burak@example.com"}}"#,
        )
        .unwrap();
        cs.add_profile(
            Tool::Codex,
            "work",
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                label: None,
            },
        )
        .unwrap();

        let err = add_oauth_with(
            &ps,
            &cs,
            "alias",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(err.to_string().contains("already exists as 'work'"));
        assert!(!ps.exists(Tool::Codex, "alias"));
    }
}
