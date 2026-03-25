use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::config::{AuthMethod, ConfigStore, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::Tool;

const CREDENTIALS_FILE: &str = ".credentials.json";
const KEY_PREFIX: &str = "sk-ant-";
const KEY_MIN_LEN: usize = 40;
const OAUTH_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub fn add_api_key(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    key: &str,
    label: Option<String>,
) -> Result<()> {
    validate_api_key(key)?;

    profile_store.create(Tool::Claude, name)?;

    let credentials = format!("{{\"apiKey\":\"{}\"}}", key);
    profile_store
        .write_file(Tool::Claude, name, CREDENTIALS_FILE, credentials.as_bytes())
        .inspect_err(|_| {
            // Best-effort cleanup on write failure.
            let _ = profile_store.delete(Tool::Claude, name);
        })?;

    config_store.add_profile(
        Tool::Claude,
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
    if !key.starts_with(KEY_PREFIX) {
        bail!("invalid Claude API key: must start with '{}'", KEY_PREFIX);
    }
    if key.len() < KEY_MIN_LEN {
        bail!(
            "invalid Claude API key: too short (minimum {} characters)",
            KEY_MIN_LEN
        );
    }
    Ok(())
}

/// Start the Claude OAuth flow using the installed `claude` binary.
///
/// Spawns `claude` with `CLAUDE_CONFIG_DIR` set to the profile directory so
/// Claude writes its credentials there rather than the default location.
/// Polls for `.credentials.json` until it appears or the timeout expires.
pub fn add_oauth(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    claude_bin: &Path,
) -> Result<()> {
    add_oauth_with(
        profile_store,
        config_store,
        name,
        label,
        claude_bin,
        OAUTH_TIMEOUT,
        POLL_INTERVAL,
    )
}

fn add_oauth_with(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    claude_bin: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<()> {
    let profile_dir = profile_store.create(Tool::Claude, name)?;

    let result =
        run_oauth_flow(claude_bin, &profile_dir, timeout, poll_interval).inspect_err(|_| {
            let _ = profile_store.delete(Tool::Claude, name);
        })?;

    set_credentials_permissions(&result)?;

    config_store.add_profile(
        Tool::Claude,
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
    claude_bin: &Path,
    profile_dir: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<PathBuf> {
    let mut child = Command::new(claude_bin)
        .env("CLAUDE_CONFIG_DIR", profile_dir)
        .spawn()
        .with_context(|| format!("could not spawn {}", claude_bin.display()))?;

    let credentials_path = profile_dir.join(CREDENTIALS_FILE);
    let deadline = Instant::now() + timeout;

    loop {
        if credentials_path.exists() {
            // Give the binary a moment to finish writing, then kill it if still running.
            let _ = child.kill();
            let _ = child.wait();
            return Ok(credentials_path);
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "Claude login timed out after {}s. \
                 The browser window may still be open.",
                timeout.as_secs()
            );
        }

        std::thread::sleep(poll_interval);
    }
}

#[cfg(unix)]
fn set_credentials_permissions(path: &Path) -> Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_credentials_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// Read the stored API key from a profile's credentials file.
pub fn read_api_key(profile_store: &ProfileStore, name: &str) -> Result<String> {
    let bytes = profile_store.read_file(Tool::Claude, name, CREDENTIALS_FILE)?;
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("could not parse credentials file: {}", e))?;
    json["apiKey"]
        .as_str()
        .map(|s| s.to_owned())
        .ok_or_else(|| anyhow::anyhow!("credentials file missing 'apiKey' field"))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;

    fn valid_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn stores(dir: &std::path::Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
    }

    #[test]
    fn validate_accepts_valid_key() {
        assert!(validate_api_key(valid_key()).is_ok());
    }

    #[test]
    fn validate_rejects_wrong_prefix() {
        let err = validate_api_key("sk-openai-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap_err();
        assert!(err.to_string().contains("sk-ant-"));
    }

    #[test]
    fn validate_rejects_too_short() {
        let err = validate_api_key("sk-ant-short").unwrap_err();
        assert!(err.to_string().contains("too short"));
    }

    #[test]
    fn add_api_key_creates_profile_and_credentials() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());

        add_api_key(&ps, &cs, "work", valid_key(), None).unwrap();

        assert!(ps.exists(Tool::Claude, "work"));
        let config = cs.load().unwrap();
        assert!(config.profiles.claude.contains_key("work"));
        assert_eq!(
            config.profiles.claude["work"].auth_method,
            AuthMethod::ApiKey
        );
    }

    #[test]
    fn add_api_key_stores_label() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());

        add_api_key(&ps, &cs, "work", valid_key(), Some("My work key".into())).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles.claude["work"].label.as_deref(),
            Some("My work key")
        );
    }

    #[test]
    fn add_api_key_credentials_file_contains_key() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());

        add_api_key(&ps, &cs, "work", valid_key(), None).unwrap();

        let key = read_api_key(&ps, "work").unwrap();
        assert_eq!(key, valid_key());
    }

    #[test]
    #[cfg(unix)]
    fn credentials_file_has_600_permissions() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "work", valid_key(), None).unwrap();

        let path = ps.profile_dir(Tool::Claude, "work").join(CREDENTIALS_FILE);
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn duplicate_profile_name_errors() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());

        add_api_key(&ps, &cs, "work", valid_key(), None).unwrap();
        let err = add_api_key(&ps, &cs, "work", valid_key(), None).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn invalid_key_format_errors_before_creating_profile() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());

        add_api_key(&ps, &cs, "work", "bad-key", None).unwrap_err();

        // Profile dir must NOT have been created.
        assert!(!ps.exists(Tool::Claude, "work"));
    }

    // ---- OAuth tests ----

    // Poll interval used in all OAuth tests: fast enough to complete quickly without
    // being sensitive to OS scheduling jitter.
    const TEST_POLL: Duration = Duration::from_millis(10);

    /// Creates a mock binary that either writes credentials immediately or exits
    /// without writing anything (for timeout tests).
    ///
    /// No `sleep` is used — `sleep` spawns a child process that becomes an orphan
    /// when the parent shell is SIGKILL'd, which can cause ETXTBSY on path reuse.
    #[cfg(unix)]
    fn make_oauth_mock(dir: &std::path::Path, write_creds: bool) -> PathBuf {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let bin = dir.join("claude");
        let body = if write_creds {
            "echo '{\"oauthToken\":\"tok\"}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n"
        } else {
            "exit 0\n" // exits without writing credentials; poll loop times out naturally
        };
        fs::write(&bin, format!("#!/bin/sh\n{}", body)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_succeeds_when_credentials_appear() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(ps.exists(Tool::Claude, "work"));
        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles.claude["work"].auth_method,
            AuthMethod::OAuth
        );
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_times_out_when_no_credentials() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        // Mock exits immediately without writing credentials.  The poll loop
        // checks for the file (not whether the child is running), so it keeps
        // polling until the timeout fires — no long-lived child processes.
        let bin = make_oauth_mock(&bin_dir, false);

        let (ps, cs) = stores(dir.path());
        let err = add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            Duration::from_millis(200),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(err.to_string().contains("timed out"));
        // Profile dir cleaned up after timeout.
        assert!(!ps.exists(Tool::Claude, "work"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_credentials_file_has_600_permissions() {
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
            "work",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let path = ps.profile_dir(Tool::Claude, "work").join(CREDENTIALS_FILE);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    #[cfg(unix)]
    fn oauth_sets_claude_config_dir_env() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        // Mock binary that writes its CLAUDE_CONFIG_DIR value to a sentinel file,
        // then writes credentials so the flow completes.
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             echo \"$CLAUDE_CONFIG_DIR\" > \"$CLAUDE_CONFIG_DIR/env_was_set\"\n\
             echo '{}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let sentinel = ps.profile_dir(Tool::Claude, "work").join("env_was_set");
        assert!(
            sentinel.exists(),
            "CLAUDE_CONFIG_DIR was not set in spawned process"
        );
    }
}
