//! Claude Code authentication — profile management, OAuth, and API key support.
//!
//! The implementation is split across focused sub-modules:
//!
//! * [`paths`]    — resolves live credential and metadata file paths
//! * [`keychain`] — auth-storage detection and OS keychain read/write
//! * [`api_key`]  — API key add, validate, and read
//! * [`oauth`]    — OAuth capture flow and account-metadata persistence

mod api_key;
mod keychain;
mod oauth;
mod paths;

use std::path::Path;

use anyhow::{Context, Result};
use std::path::PathBuf;

use super::files;
use super::secure_store;
use crate::config::CredentialBackend;
use crate::profile::ProfileStore;
use crate::types::{StateMode, Tool};

use keychain::{auth_storage, ClaudeAuthStorage};
use paths::live_credentials_path;

// ---- Constants ----

pub(super) const CREDENTIALS_FILE: &str = ".credentials.json";
pub(super) const OAUTH_ACCOUNT_FILE: &str = "oauth-account.json";
pub(super) const OAUTH_CAPTURE_DIR: &str = ".oauth-capture";
pub(super) const OAUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
pub(super) const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
pub(super) const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
pub(super) const KEYCHAIN_BACKEND: super::secure_backend::SecureBackend =
    super::secure_backend::SecureBackend::SystemKeyring;

// ---- Public types ----

pub enum LiveCredentialSource {
    File(PathBuf),
    Keychain,
}

pub struct LiveCredentialSnapshot {
    pub bytes: Vec<u8>,
    pub source: LiveCredentialSource,
}

// ---- Public re-exports from sub-modules ----

pub use api_key::{add_api_key, read_api_key, validate_api_key};
pub use keychain::{
    imported_profile_backend, keychain_import_supported, preferred_import_backend,
    read_live_keychain_credentials_for_import, storage_fallback_note, uses_live_keychain,
};
pub use oauth::{
    add_oauth, capture_live_oauth_account_metadata, live_credentials_snapshot_for_import,
    read_live_oauth_account_metadata_for_import,
};
pub use paths::live_local_state_dir;

// ---- Core public functions ----

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
            keychain::write_keychain_credentials(&keychain::live_keychain_payload(&stored))
        }
    }?;

    oauth::apply_live_oauth_account_metadata(profile_store, name, user_home)
}

pub fn emit_shell_env(name: &str, profile_store: &ProfileStore, mode: StateMode) {
    match mode {
        StateMode::Isolated => {
            let profile_dir = profile_store.profile_dir(Tool::Claude, name);
            files::emit_export("CLAUDE_CONFIG_DIR", &profile_dir.display().to_string());
        }
        StateMode::Shared => {
            files::emit_unset("CLAUDE_CONFIG_DIR");
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
            let Some(live) = keychain::read_keychain_credentials()? else {
                return Ok(false);
            };
            // The Keychain only stores the claudeAiOauth subset (written by
            // live_keychain_payload). Compare as parsed JSON values to handle
            // the trailing newline added by the security CLI and key ordering.
            let live_value = serde_json::from_slice::<serde_json::Value>(&live)
                .context("could not parse live Keychain credentials")?;
            let stored_payload = keychain::live_keychain_payload(&stored);
            let stored_value = serde_json::from_slice::<serde_json::Value>(&stored_payload)
                .context("could not parse stored credential payload")?;
            Ok(live_value == stored_value)
        }
    }
}

pub(super) fn read_stored_credentials(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
) -> Result<Vec<u8>> {
    match backend {
        CredentialBackend::File => profile_store.read_file(Tool::Claude, name, CREDENTIALS_FILE),
        CredentialBackend::SystemKeyring => secure_store::read_profile_secret(Tool::Claude, name)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "secure credentials for Claude Code profile '{}' are missing from the system keyring",
                    name
                )
            }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    use super::*;
    use crate::auth::secure_backend;
    use crate::auth::secure_store;
    use crate::auth::test_overrides::EnvVarGuard;
    use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
    use crate::profile::ProfileStore;
    use chrono::Utc;

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
                 -T) shift ;;\n\
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
    const TEST_POLL: std::time::Duration = std::time::Duration::from_millis(10);

    /// Creates a mock binary that either writes credentials immediately or exits
    /// without writing anything (for timeout tests).
    ///
    /// No `sleep` is used — `sleep` spawns a child process that becomes an orphan
    /// when the parent shell is SIGKILL'd, which can cause ETXTBSY on path reuse.
    #[cfg(all(unix, not(target_os = "macos")))]
    fn make_oauth_mock(dir: &std::path::Path, write_creds: bool) -> PathBuf {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let bin = dir.join("claude");
        let body = if write_creds {
            "[ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             mkdir -p \"$HOME/.claude\"\n\
             echo '{\"oauthToken\":\"tok\"}' > \"$HOME/.claude/.credentials.json\"\n\
             exit 0\n"
        } else {
            "[ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             exit 0\n"
        };
        fs::write(&bin, format!("#!/bin/sh\n{}", body)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn oauth_flow_succeeds_when_credentials_appear() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&bin_dir).unwrap();
        let _home = EnvVarGuard::set("HOME", &home);
        let bin = make_oauth_mock(&bin_dir, true);

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            std::time::Duration::from_secs(2),
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
    #[cfg(all(unix, not(target_os = "macos")))]
    fn oauth_duplicate_identity_is_rejected_and_cleaned_up() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        let _home = EnvVarGuard::set("HOME", &home);
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             mkdir -p \"$HOME/.claude\"\n\
             echo '{\"oauthToken\":\"tok\",\"account\":{\"email\":\"burak@example.com\"}}' > \"$HOME/.claude/.credentials.json\"\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            CREDENTIALS_FILE,
            br#"{"oauthToken":"tok","account":{"email":"burak@example.com"}}"#,
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

        let err = oauth::add_oauth_with(
            &ps,
            &cs,
            "alias",
            None,
            &bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(err.to_string().contains("already exists as 'work'"));
        assert!(!ps.exists(Tool::Claude, "alias"));
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn oauth_flow_errors_when_claude_exits_without_credentials() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&bin_dir).unwrap();
        let _home = EnvVarGuard::set("HOME", &home);
        // Mock exits immediately without writing credentials so the OAuth flow
        // reports an actionable capture failure instead of hanging.
        let bin = make_oauth_mock(&bin_dir, false);

        let (ps, cs) = stores(dir.path());
        let err = oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            std::time::Duration::from_millis(200),
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
    #[cfg(all(unix, not(target_os = "macos")))]
    fn oauth_credentials_file_has_600_permissions() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&bin_dir).unwrap();
        let _home = EnvVarGuard::set("HOME", &home);
        let bin = make_oauth_mock(&bin_dir, true);

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let path = ps.profile_dir(Tool::Claude, "work").join(CREDENTIALS_FILE);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn oauth_keychain_capture_persists_managed_credentials_file() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        let security_bin = bin_dir.join("security");
        write_security_mock(&security_bin);
        let claude_bin = bin_dir.join("claude");
        fs::write(
            &claude_bin,
            "#!/bin/sh\n\
             item=\"$AISW_KEYRING_TEST_DIR/Claude Code-credentials/${USER:-tester}\"\n\
             mkdir -p \"$item\"\n\
             printf '%s' \"${USER:-tester}\" > \"$item/account\"\n\
             printf '%s' '{\"account\":{\"email\":\"work@example.com\"}}' > \"$item/secret\"\n",
        )
        .unwrap();
        fs::set_permissions(&claude_bin, fs::Permissions::from_mode(0o755)).unwrap();

        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));
        let _security = EnvVarGuard::set(
            "AISW_SECURITY_BIN",
            security_bin
                .to_str()
                .expect("security path should be utf-8"),
        );
        let _user = EnvVarGuard::set("USER", "tester");

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &claude_bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].credential_backend,
            CredentialBackend::File
        );
        assert_eq!(
            fs::read(ps.profile_dir(Tool::Claude, "work").join(CREDENTIALS_FILE)).unwrap(),
            br#"{"account":{"email":"work@example.com"}}"#
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn oauth_flow_accepts_existing_keychain_credentials_after_successful_login() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        let security_bin = bin_dir.join("security");
        write_security_mock(&security_bin);
        let claude_bin = bin_dir.join("claude");
        fs::write(
            &claude_bin,
            "#!/bin/sh\n\
             [ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&claude_bin, fs::Permissions::from_mode(0o755)).unwrap();

        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));
        let _security = EnvVarGuard::set(
            "AISW_SECURITY_BIN",
            security_bin
                .to_str()
                .expect("security path should be utf-8"),
        );
        let _user = EnvVarGuard::set("USER", "tester");

        let existing = br#"{"account":{"email":"work@example.com"}}"#;
        keychain::write_keychain_credentials(existing).unwrap();

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &claude_bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert_eq!(
            ps.read_file(Tool::Claude, "work", CREDENTIALS_FILE)
                .unwrap(),
            existing
        );
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn oauth_on_non_macos_avoids_claude_config_dir_during_login() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        let _home = EnvVarGuard::set("HOME", &home);
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             [ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             [ -z \"$CLAUDE_CONFIG_DIR\" ] || { echo \"$CLAUDE_CONFIG_DIR\" > \"$HOME/env_was_set\"; exit 7; }\n\
             mkdir -p \"$HOME/.claude\"\n\
             echo '{}' > \"$HOME/.claude/.credentials.json\"\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(
            !home.join("env_was_set").exists(),
            "CLAUDE_CONFIG_DIR should not be set during non-macOS OAuth login"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn oauth_on_macos_avoids_claude_config_dir_and_reads_live_file() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let _home = EnvVarGuard::set("HOME", &home);
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             [ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             [ -z \"$CLAUDE_CONFIG_DIR\" ] || { echo \"$CLAUDE_CONFIG_DIR\" > \"$HOME/env_was_set\"; exit 7; }\n\
             mkdir -p \"$HOME/.claude\"\n\
             echo '{\"oauthToken\":\"tok\"}' > \"$HOME/.claude/.credentials.json\"\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(
            !home.join("env_was_set").exists(),
            "CLAUDE_CONFIG_DIR should not be set during macOS OAuth login"
        );
        let stored = ps
            .read_file(Tool::Claude, "work", CREDENTIALS_FILE)
            .unwrap();
        assert_eq!(
            String::from_utf8(stored).unwrap().trim(),
            r#"{"oauthToken":"tok"}"#
        );
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn oauth_uses_auth_login_subcommand() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        let _home = EnvVarGuard::set("HOME", &home);
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             [ -z \"$CLAUDE_CONFIG_DIR\" ] || exit 7\n\
             printf '%s %s' \"$1\" \"$2\" > \"$HOME/login_args\"\n\
             mkdir -p \"$HOME/.claude\"\n\
             echo '{}' > \"$HOME/.claude/.credentials.json\"\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(home.join("login_args")).unwrap(),
            "auth login"
        );
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn oauth_on_non_macos_accepts_live_credentials_when_capture_dir_is_ignored() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        let _home = EnvVarGuard::set("HOME", &home);

        // Simulate Claude writing only to the live location, even when
        // CLAUDE_CONFIG_DIR is provided.
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             [ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             mkdir -p \"$HOME/.claude\"\n\
             echo '{\"oauthToken\":\"tok\"}' > \"$HOME/.claude/.credentials.json\"\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let stored = ps
            .read_file(Tool::Claude, "work", CREDENTIALS_FILE)
            .unwrap();
        assert_eq!(
            String::from_utf8(stored).unwrap().trim(),
            r#"{"oauthToken":"tok"}"#
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn oauth_on_macos_uses_auth_login_subcommand() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let _home = EnvVarGuard::set("HOME", &home);
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             printf '%s %s' \"$1\" \"$2\" > \"$HOME/login_args\"\n\
             mkdir -p \"$HOME/.claude\"\n\
             echo '{}' > \"$HOME/.claude/.credentials.json\"\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(home.join("login_args")).unwrap(),
            "auth login"
        );
    }

    #[test]
    fn capture_live_oauth_account_metadata_persists_profile_file() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(&user_home).unwrap();
        fs::write(
            user_home.join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"work@example.com","organizationUuid":"org-123"},"numStartups":3}"#,
        )
        .unwrap();

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();

        capture_live_oauth_account_metadata(&ps, "work", &user_home).unwrap();

        let stored = ps
            .read_file(Tool::Claude, "work", OAUTH_ACCOUNT_FILE)
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&stored).unwrap(),
            serde_json::json!({
                "emailAddress": "work@example.com",
                "organizationUuid": "org-123"
            })
        );
    }

    #[test]
    fn apply_live_credentials_updates_oauth_account_metadata() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(&user_home).unwrap();
        fs::write(
            user_home.join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"old@example.com"},"numStartups":7}"#,
        )
        .unwrap();

        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            CREDENTIALS_FILE,
            br#"{"claudeAiOauth":{"accessToken":"tok"},"mcpOAuth":{"x":{"clientId":"abc"}}}"#,
        )
        .unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            OAUTH_ACCOUNT_FILE,
            br#"{"emailAddress":"new@example.com","organizationUuid":"org-456"}"#,
        )
        .unwrap();
        secure_backend::upsert_generic_password(
            KEYCHAIN_BACKEND,
            KEYCHAIN_SERVICE,
            "tester",
            br#"{"claudeAiOauth":{"accessToken":"old"}}"#,
        )
        .unwrap();

        apply_live_credentials(&ps, "work", CredentialBackend::File, &user_home).unwrap();

        let live_keychain = keychain::read_keychain_credentials().unwrap().unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&live_keychain).unwrap(),
            serde_json::json!({
                "claudeAiOauth": {
                    "accessToken": "tok"
                }
            })
        );

        let live_metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(user_home.join(".claude.json")).unwrap()).unwrap();
        assert_eq!(
            live_metadata["oauthAccount"]["emailAddress"],
            "new@example.com"
        );
        assert_eq!(live_metadata["oauthAccount"]["organizationUuid"], "org-456");
        assert_eq!(live_metadata["numStartups"], 7);
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
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));
        let _security = EnvVarGuard::set(
            "AISW_SECURITY_BIN",
            security_bin
                .to_str()
                .expect("security path should be utf-8"),
        );

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        secure_store::write_profile_secret(Tool::Claude, "work", br#"{"token":"tok"}"#).unwrap();

        apply_live_credentials(&ps, "work", CredentialBackend::SystemKeyring, &user_home).unwrap();

        assert!(
            live_credentials_match(&ps, "work", CredentialBackend::SystemKeyring, &user_home)
                .unwrap()
        );
    }
}
