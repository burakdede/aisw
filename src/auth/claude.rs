use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;

use super::files;
use super::identity;
use super::macos_keychain;
use super::secure_store;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::{StateMode, Tool};

const CREDENTIALS_FILE: &str = ".credentials.json";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaudeAuthStorage {
    File,
    Keychain,
}

pub enum LiveCredentialSource {
    File(PathBuf),
    Keychain,
}

pub struct LiveCredentialSnapshot {
    pub bytes: Vec<u8>,
    pub source: LiveCredentialSource,
}

fn live_credentials_path(user_home: &Path) -> PathBuf {
    let primary = user_home.join(".claude").join(CREDENTIALS_FILE);
    let secondary = user_home
        .join(".config")
        .join("claude")
        .join(CREDENTIALS_FILE);

    if secondary.exists() && !primary.exists() {
        secondary
    } else {
        primary
    }
}

pub fn live_local_state_dir(user_home: &Path) -> Option<PathBuf> {
    let primary = user_home.join(".claude");
    if primary.exists() {
        return Some(primary);
    }

    let secondary = user_home.join(".config").join("claude");
    if secondary.exists() {
        Some(secondary)
    } else {
        None
    }
}

fn auth_storage(user_home: &Path) -> ClaudeAuthStorage {
    if let Some(storage) = forced_auth_storage() {
        return storage;
    }

    if live_credentials_path(user_home).exists() {
        ClaudeAuthStorage::File
    } else if cfg!(target_os = "macos") && read_keychain_credentials().ok().flatten().is_some() {
        ClaudeAuthStorage::Keychain
    } else {
        ClaudeAuthStorage::File
    }
}

fn forced_auth_storage() -> Option<ClaudeAuthStorage> {
    match super::test_overrides::string("AISW_CLAUDE_AUTH_STORAGE").as_deref() {
        Some("file") => Some(ClaudeAuthStorage::File),
        Some("keychain") => Some(ClaudeAuthStorage::Keychain),
        _ => None,
    }
}

pub fn keychain_import_supported() -> bool {
    forced_auth_storage() == Some(ClaudeAuthStorage::Keychain) || cfg!(target_os = "macos")
}

fn watch_keychain_during_oauth() -> bool {
    match forced_auth_storage() {
        Some(ClaudeAuthStorage::File) => false,
        Some(ClaudeAuthStorage::Keychain) => true,
        None => cfg!(target_os = "macos"),
    }
}

fn keychain_account() -> String {
    macos_keychain::find_generic_password_account(KEYCHAIN_SERVICE)
        .ok()
        .flatten()
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "aisw".to_owned())
}

fn read_keychain_credentials() -> Result<Option<Vec<u8>>> {
    macos_keychain::read_generic_password(KEYCHAIN_SERVICE, None)
        .context("could not query macOS Keychain for Claude Code credentials")
}

pub fn read_live_keychain_credentials_for_import() -> Result<Option<Vec<u8>>> {
    if keychain_import_supported() {
        read_keychain_credentials()
    } else {
        Ok(None)
    }
}

pub fn live_credentials_snapshot_for_import(
    user_home: &Path,
) -> Result<Option<LiveCredentialSnapshot>> {
    let live_path = live_credentials_path(user_home);
    if live_path.exists() {
        let bytes = std::fs::read(&live_path)
            .with_context(|| format!("could not read {}", live_path.display()))?;
        return Ok(Some(LiveCredentialSnapshot {
            bytes,
            source: LiveCredentialSource::File(live_path),
        }));
    }

    if live_local_state_dir(user_home).is_none() {
        return Ok(None);
    }

    let Some(bytes) = read_live_keychain_credentials_for_import()? else {
        return Ok(None);
    };

    Ok(Some(LiveCredentialSnapshot {
        bytes,
        source: LiveCredentialSource::Keychain,
    }))
}

fn write_keychain_credentials(bytes: &[u8]) -> Result<()> {
    macos_keychain::upsert_generic_password(KEYCHAIN_SERVICE, &keychain_account(), bytes)
        .context("could not write Claude Code credentials into macOS Keychain")
}

fn capture_keychain_credentials(profile_dir: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let path = profile_dir.join(CREDENTIALS_FILE);
    std::fs::write(&path, bytes).with_context(|| format!("could not write {}", path.display()))?;
    files::set_permissions_600(&path)?;
    Ok(path)
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
        Tool::Claude,
        key,
    )? {
        bail!(
            "Claude Code API key already exists as profile '{}'.\n  \
             Use that profile or provide a different API key.",
            existing_name
        );
    }

    profile_store.create(Tool::Claude, name)?;

    let credentials = format!("{{\"apiKey\":\"{}\"}}", key);
    files::cleanup_profile_on_error(
        profile_store.write_file(Tool::Claude, name, CREDENTIALS_FILE, credentials.as_bytes()),
        profile_store,
        Tool::Claude,
        name,
    )?;

    config_store.add_profile(
        Tool::Claude,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::ApiKey,
            credential_backend: CredentialBackend::File,
            label,
        },
    )?;

    Ok(())
}

pub fn validate_api_key(key: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!(
            "Claude API key must not be empty.\n  \
             Get your API key at console.anthropic.com → API Keys.",
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

    let result = files::cleanup_profile_on_error(
        run_oauth_flow(claude_bin, &profile_dir, timeout, poll_interval),
        profile_store,
        Tool::Claude,
        name,
    )?;

    files::set_permissions_600(&result)?;
    files::cleanup_profile_on_error(
        identity::ensure_unique_oauth_identity(
            profile_store,
            config_store,
            Tool::Claude,
            name,
            CredentialBackend::File,
        ),
        profile_store,
        Tool::Claude,
        name,
    )?;

    config_store.add_profile(
        Tool::Claude,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            credential_backend: CredentialBackend::File,
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
    let forced_storage = forced_auth_storage();
    let mut watch_keychain = watch_keychain_during_oauth();
    let keychain_before = if watch_keychain {
        match read_keychain_credentials() {
            Ok(credentials) => credentials,
            Err(_err) if forced_storage != Some(ClaudeAuthStorage::Keychain) => {
                watch_keychain = false;
                None
            }
            Err(err) => return Err(err),
        }
    } else {
        None
    };

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

        if watch_keychain {
            if let Some(current) = read_keychain_credentials()? {
                let changed = keychain_before.as_deref() != Some(current.as_slice());
                if changed {
                    let _ = child.kill();
                    let _ = child.wait();
                    return capture_keychain_credentials(profile_dir, &current);
                }
            }
        }

        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("could not poll {}", claude_bin.display()))?
        {
            if watch_keychain {
                if let Some(current) = read_keychain_credentials()? {
                    if keychain_before.is_none() {
                        return capture_keychain_credentials(profile_dir, &current);
                    }
                }
            }

            let exit_note = if status.success() {
                "Claude exited"
            } else {
                "Claude exited with an error"
            };
            bail!(
                "{} before aisw could capture credentials.\n  \
                 On this platform Claude may be storing auth outside CLAUDE_CONFIG_DIR.",
                exit_note
            );
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

/// Read the stored API key from a profile's credentials file.
pub fn read_api_key(profile_store: &ProfileStore, name: &str) -> Result<String> {
    let bytes = profile_store.read_file(Tool::Claude, name, CREDENTIALS_FILE)?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        anyhow::anyhow!(
            "could not parse credentials file for profile '{}'.\n  \
             The profile may be corrupted. Run 'aisw remove claude {}' \
             then 'aisw add claude {}' to reconfigure.\n  \
             ({})",
            name,
            name,
            name,
            e
        )
    })?;
    json["apiKey"]
        .as_str()
        .map(|s| s.to_owned())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "credentials file for profile '{}' is missing the 'apiKey' field.\n  \
                 Run 'aisw remove claude {}' then 'aisw add claude {}' to reconfigure.",
                name,
                name,
                name
            )
        })
}

pub fn apply_live_credentials(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<()> {
    match auth_storage(user_home) {
        ClaudeAuthStorage::File => files::apply_profile_file(
            profile_store,
            Tool::Claude,
            name,
            CREDENTIALS_FILE,
            live_credentials_path(user_home),
        ),
        ClaudeAuthStorage::Keychain => {
            let stored = read_stored_credentials(profile_store, name, backend)?;
            write_keychain_credentials(&stored)
        }
    }
}

pub fn emit_shell_env(name: &str, profile_store: &ProfileStore, mode: StateMode) {
    match mode {
        StateMode::Isolated => {
            let profile_dir = profile_store.profile_dir(Tool::Claude, name);
            println!(
                "export CLAUDE_CONFIG_DIR={}",
                shell_single_quote(&profile_dir.display().to_string())
            );
        }
        StateMode::Shared => {
            println!("unset CLAUDE_CONFIG_DIR");
        }
    }
}

pub fn live_credentials_match(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<bool> {
    let stored = read_stored_credentials(profile_store, name, backend)?;
    match auth_storage(user_home) {
        ClaudeAuthStorage::File => {
            let live_path = live_credentials_path(user_home);
            if !live_path.exists() {
                return Ok(false);
            }
            let live = std::fs::read(&live_path)
                .with_context(|| format!("could not read {}", live_path.display()))?;
            Ok(live == stored)
        }
        ClaudeAuthStorage::Keychain => {
            let Some(live) = read_keychain_credentials()? else {
                return Ok(false);
            };
            Ok(live == stored)
        }
    }
}

fn read_stored_credentials(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
) -> Result<Vec<u8>> {
    match backend {
        CredentialBackend::File => profile_store.read_file(Tool::Claude, name, CREDENTIALS_FILE),
        CredentialBackend::MacosKeychain => secure_store::read_profile_secret(Tool::Claude, name)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "secure credentials for Claude Code profile '{}' are missing from macOS Keychain",
                    name
                )
            }),
    }
}

fn shell_single_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    use super::*;
    use crate::auth::secure_store;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;

    fn valid_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn stores(dir: &std::path::Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
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
                 if [ \"$service\" = \"aisw\" ]; then key=\"$service-$account\"; else key=\"$service\"; fi\n\
                 key=$(printf '%s' \"$key\" | tr ' /:' '___')\n\
                 store=\"$HOME/$key.json\"\n\
                 if [ -f \"$store\" ]; then\n\
                   cat \"$store\"\n\
                   exit 0\n\
                 fi\n\
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
                     -w) shift; secret=\"$1\" ;;\n\
                   esac\n\
                   shift\n\
                 done\n\
                 if [ \"$service\" = \"aisw\" ]; then key=\"$service-$account\"; else key=\"$service\"; fi\n\
                 key=$(printf '%s' \"$key\" | tr ' /:' '___')\n\
                 store=\"$HOME/$key.json\"\n\
                 printf '%s' \"$secret\" > \"$store\"\n\
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
                 if [ \"$service\" = \"aisw\" ]; then key=\"$service-$account\"; else key=\"$service\"; fi\n\
                 key=$(printf '%s' \"$key\" | tr ' /:' '___')\n\
                 store=\"$HOME/$key.json\"\n\
                 rm -f \"$store\"\n\
                 exit 0\n\
                 ;;\n\
             esac\n\
             exit 1\n",
        )
        .unwrap();
        fs::set_permissions(bin, fs::Permissions::from_mode(0o755)).unwrap();
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            // Tests that mutate this hold SPAWN_LOCK, so process-wide env access stays serialized.
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

    #[test]
    fn validate_accepts_valid_key() {
        assert!(validate_api_key(valid_key()).is_ok());
    }

    #[test]
    fn validate_rejects_empty_key() {
        let err = validate_api_key("").unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
        assert!(err.to_string().contains("console.anthropic.com"));
    }

    #[test]
    fn validate_rejects_whitespace_only_key() {
        let err = validate_api_key("   ").unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
        assert!(err.to_string().contains("console.anthropic.com"));
    }

    #[test]
    fn read_api_key_corrupted_json_error_mentions_reconfigure() {
        let dir = tempdir().unwrap();
        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(Tool::Claude, "work", CREDENTIALS_FILE, b"not json")
            .unwrap();
        let err = read_api_key(&ps, "work").unwrap_err();
        assert!(err.to_string().contains("aisw remove claude work"));
        assert!(err.to_string().contains("aisw add claude work"));
    }

    #[test]
    fn read_api_key_missing_field_error_mentions_reconfigure() {
        let dir = tempdir().unwrap();
        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            CREDENTIALS_FILE,
            b"{\"other\":\"val\"}",
        )
        .unwrap();
        let err = read_api_key(&ps, "work").unwrap_err();
        assert!(err.to_string().contains("aisw remove claude work"));
        assert!(err.to_string().contains("aisw add claude work"));
    }

    #[test]
    fn add_api_key_creates_profile_and_credentials() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());

        add_api_key(&ps, &cs, "work", valid_key(), None).unwrap();

        assert!(ps.exists(Tool::Claude, "work"));
        let config = cs.load().unwrap();
        assert!(config.profiles_for(Tool::Claude).contains_key("work"));
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].auth_method,
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
            config.profiles_for(Tool::Claude)["work"].label.as_deref(),
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
    fn empty_key_errors_before_creating_profile() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());

        add_api_key(&ps, &cs, "work", "   ", None).unwrap_err();

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
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
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
            config.profiles_for(Tool::Claude)["work"].auth_method,
            AuthMethod::OAuth
        );
    }

    #[test]
    #[cfg(unix)]
    fn oauth_duplicate_identity_is_rejected_and_cleaned_up() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             echo '{\"account\":{\"email\":\"burak@example.com\"}}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            CREDENTIALS_FILE,
            br#"{"account":{"email":"burak@example.com"}}"#,
        )
        .unwrap();
        cs.add_profile(
            Tool::Claude,
            "work",
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: CredentialBackend::File,
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
        assert!(!ps.exists(Tool::Claude, "alias"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_errors_when_claude_exits_without_credentials() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        // Mock exits immediately without writing credentials so the OAuth flow
        // reports an actionable capture failure instead of hanging.
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

        let message = err.to_string();
        assert!(
            message.contains("exited before aisw could capture credentials")
                || message.contains("timed out"),
            "unexpected error: {message}"
        );
        // Profile dir cleaned up after the failed OAuth attempt.
        assert!(!ps.exists(Tool::Claude, "work"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_credentials_file_has_600_permissions() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
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
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
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

    #[test]
    fn keychain_backed_profile_applies_and_matches_live_keychain() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        let user_home = dir.path().join("home");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let security_bin = bin_dir.join("security");
        write_security_mock(&security_bin);

        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _security = EnvVarGuard::set(
            "AISW_SECURITY_BIN",
            security_bin
                .to_str()
                .expect("security path should be utf-8"),
        );

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        secure_store::write_profile_secret(Tool::Claude, "work", br#"{"token":"tok"}"#).unwrap();

        apply_live_credentials(&ps, "work", CredentialBackend::MacosKeychain, &user_home).unwrap();

        assert!(
            live_credentials_match(&ps, "work", CredentialBackend::MacosKeychain, &user_home)
                .unwrap()
        );
    }
}
