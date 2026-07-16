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

use keychain::{
    auth_storage, current_keychain_scheme, keychain_service_for_config_dir, ClaudeAuthStorage,
    ClaudeKeychainScheme as KeychainScheme,
};
use paths::live_credentials_path;

// ---- Constants ----

pub(super) const CREDENTIALS_FILE: &str = ".credentials.json";
pub(super) const OAUTH_ACCOUNT_FILE: &str = "oauth-account.json";
pub(super) const OAUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
pub(super) const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
pub(super) const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
pub(super) const KEYCHAIN_BACKEND: super::secure_backend::SecureBackend =
    super::secure_backend::SecureBackend::SystemKeyring;

// ---- Public types ----

#[derive(Debug, Clone)]
pub enum LiveCredentialSource {
    File(PathBuf),
    Keychain,
}

#[derive(Debug, Clone)]
pub struct LiveCredentialSnapshot {
    pub bytes: Vec<u8>,
    pub source: LiveCredentialSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeAuthClassification {
    ApiKey,
    OAuthFileBacked,
    OAuthKeychainScopedByConfigDir,
    OAuthMacosKeychainSharedLive,
    OAuthKeychainUnknown,
}

impl ClaudeAuthClassification {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaudeAuthClassification::ApiKey => "api_key",
            ClaudeAuthClassification::OAuthFileBacked => "oauth_file_backed",
            ClaudeAuthClassification::OAuthKeychainScopedByConfigDir => {
                "oauth_keychain_scoped_by_config_dir"
            }
            ClaudeAuthClassification::OAuthMacosKeychainSharedLive => {
                "oauth_macos_keychain_shared_live"
            }
            ClaudeAuthClassification::OAuthKeychainUnknown => "oauth_keychain_unknown",
        }
    }

    pub fn human_label(self) -> &'static str {
        match self {
            ClaudeAuthClassification::ApiKey => "API key",
            ClaudeAuthClassification::OAuthFileBacked => "OAuth file-backed",
            ClaudeAuthClassification::OAuthKeychainScopedByConfigDir => {
                "OAuth keychain scoped by config dir"
            }
            ClaudeAuthClassification::OAuthMacosKeychainSharedLive => "OAuth macOS shared Keychain",
            ClaudeAuthClassification::OAuthKeychainUnknown => "OAuth keychain unknown",
        }
    }

    pub fn blocks_isolated_mode(self) -> bool {
        matches!(self, ClaudeAuthClassification::OAuthMacosKeychainSharedLive)
    }
}

// ---- Public re-exports from sub-modules ----

pub use api_key::{
    add_api_key, add_api_key_with_backend, read_api_key, read_api_key_with_backend,
    validate_api_key,
};
pub use keychain::ClaudeKeychainScheme;
pub use keychain::{
    current_keychain_scheme as current_claude_keychain_scheme,
    detected_keychain_scheme as detected_claude_keychain_scheme, imported_profile_backend,
    keychain_import_supported, oauth_stored_backend, preferred_import_backend,
    read_live_keychain_credentials_for_import, storage_fallback_note, uses_live_keychain,
};
pub use oauth::{
    add_oauth, add_oauth_with_backend, capture_live_oauth_account_metadata,
    live_credentials_snapshot_for_import, read_live_oauth_account_metadata_for_import,
    restore_live_state_after_oauth_add, sync_profile_from_live_if_same_identity,
};
pub use paths::live_local_state_dir;

pub fn classify_profile(
    user_home: &Path,
    profile_store: &ProfileStore,
    name: &str,
    auth_method: crate::config::AuthMethod,
    _backend: CredentialBackend,
) -> Result<ClaudeAuthClassification> {
    Ok(match auth_method {
        crate::config::AuthMethod::ApiKey => ClaudeAuthClassification::ApiKey,
        crate::config::AuthMethod::OAuth => {
            if uses_live_keychain(user_home) {
                match current_keychain_scheme() {
                    KeychainScheme::LegacyShared => {
                        ClaudeAuthClassification::OAuthMacosKeychainSharedLive
                    }
                    KeychainScheme::ScopedByConfigDir => {
                        let profile_dir = profile_store.profile_dir(Tool::Claude, name);
                        let service = keychain_service_for_config_dir(
                            &profile_dir,
                            user_home,
                            KeychainScheme::ScopedByConfigDir,
                        );
                        if service == KEYCHAIN_SERVICE {
                            ClaudeAuthClassification::OAuthMacosKeychainSharedLive
                        } else {
                            ClaudeAuthClassification::OAuthKeychainScopedByConfigDir
                        }
                    }
                    KeychainScheme::Unknown => ClaudeAuthClassification::OAuthKeychainUnknown,
                }
            } else {
                ClaudeAuthClassification::OAuthFileBacked
            }
        }
    })
}

pub fn login_targets_profile_state(user_home: &Path) -> bool {
    !uses_live_keychain(user_home)
        || matches!(current_keychain_scheme(), KeychainScheme::ScopedByConfigDir)
}

// ---- Core public functions ----

pub fn apply_live_credentials(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
    user_home: &Path,
    state_mode: StateMode,
) -> Result<()> {
    let stored = read_stored_credentials(profile_store, name, backend)?;

    match auth_storage(user_home) {
        ClaudeAuthStorage::File => {
            crate::live_apply::apply_transaction(vec![crate::live_apply::LiveFileChange::write(
                live_credentials_path(user_home),
                stored,
            )])
        }
        ClaudeAuthStorage::Keychain => {
            let service = match state_mode {
                StateMode::Shared => KEYCHAIN_SERVICE.to_owned(),
                StateMode::Isolated => keychain_service_for_config_dir(
                    &profile_store.profile_dir(Tool::Claude, name),
                    user_home,
                    current_keychain_scheme(),
                ),
            };
            keychain::write_keychain_credentials_for_service(&service, &stored)
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
    state_mode: StateMode,
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
            let live_value =
                credentials_json_value(&live).context("could not parse live credentials file")?;
            let stored_value =
                credentials_json_value(&stored).context("could not parse stored credentials")?;
            Ok(live_value == stored_value)
        }
        ClaudeAuthStorage::Keychain => {
            let service = match state_mode {
                StateMode::Shared => KEYCHAIN_SERVICE.to_owned(),
                StateMode::Isolated => keychain_service_for_config_dir(
                    &profile_store.profile_dir(Tool::Claude, name),
                    user_home,
                    current_keychain_scheme(),
                ),
            };
            let Some(live) = keychain::read_keychain_credentials_for_service(&service)? else {
                return Ok(false);
            };
            // Compare as parsed JSON values to handle the trailing newline
            // added by the security CLI and any key-ordering differences.
            let live_value = credentials_json_value(&live)
                .context("could not parse live Keychain credentials")?;
            let stored_value = credentials_json_value(&stored)
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
    .map(|bytes| normalize_credentials_bytes(&bytes).unwrap_or(bytes))
}

pub(crate) fn persist_stored_credentials(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
    bytes: &[u8],
) -> Result<()> {
    let normalized = normalize_credentials_bytes(bytes).unwrap_or_else(|| bytes.to_vec());
    match backend {
        CredentialBackend::File => {
            profile_store.write_file(Tool::Claude, name, CREDENTIALS_FILE, &normalized)
        }
        CredentialBackend::SystemKeyring => {
            secure_store::write_profile_secret(Tool::Claude, name, &normalized)
        }
    }
}

pub(crate) fn normalize_credentials_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    if has_object_json_shape(bytes) {
        return Some(bytes.to_vec());
    }

    let mut candidate = bytes.to_vec();
    for _ in 0..3 {
        let text = std::str::from_utf8(&candidate).ok()?.trim();
        if text.len() % 2 != 0 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }

        let decoded = decode_hex_bytes(text)?;
        if has_object_json_shape(&decoded) {
            return Some(decoded);
        }
        candidate = decoded;
    }

    None
}

fn credentials_json_value(bytes: &[u8]) -> Result<serde_json::Value> {
    let normalized = normalize_credentials_bytes(bytes).unwrap_or_else(|| bytes.to_vec());
    serde_json::from_slice(&normalized).context("credential payload is not valid JSON")
}

fn has_object_json_shape(bytes: &[u8]) -> bool {
    matches!(
        serde_json::from_slice::<serde_json::Value>(bytes),
        Ok(serde_json::Value::Object(_))
    )
}

fn decode_hex_bytes(text: &str) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(text.len() / 2);
    let mut chunks = text.as_bytes().chunks_exact(2);
    for pair in &mut chunks {
        let hi = decode_hex_nibble(pair[0])?;
        let lo = decode_hex_nibble(pair[1])?;
        decoded.push((hi << 4) | lo);
    }
    if !chunks.remainder().is_empty() {
        return None;
    }
    Some(decoded)
}

fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use crate::auth::identity;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    use super::*;
    use crate::auth::secure_backend;
    use crate::auth::secure_store;
    use crate::auth::test_overrides::EnvVarGuard;
    #[cfg(all(unix, not(target_os = "macos")))]
    use crate::config::ProfileMeta;
    use crate::config::{AuthMethod, ConfigStore, CredentialBackend};
    use crate::profile::ProfileStore;
    #[cfg(all(unix, not(target_os = "macos")))]
    use chrono::Utc;

    fn valid_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn stores(dir: &std::path::Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
    }

    #[test]
    fn classify_api_key_profile_as_api_key() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(&user_home).unwrap();
        let classification = classify_profile(
            &user_home,
            &ProfileStore::new(dir.path()),
            "work",
            AuthMethod::ApiKey,
            CredentialBackend::File,
        )
        .unwrap();
        assert_eq!(classification, ClaudeAuthClassification::ApiKey);
    }

    #[test]
    fn classify_oauth_profile_as_shared_keychain_when_macos_keychain_is_active() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(&user_home).unwrap();
        let _platform_guard = EnvVarGuard::set("AISW_TEST_CLAUDE_PLATFORM", "macos");
        let _storage_guard = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _scheme_guard = EnvVarGuard::set("AISW_CLAUDE_KEYCHAIN_SCHEME", "shared");
        let classification = classify_profile(
            &user_home,
            &ProfileStore::new(dir.path()),
            "work",
            AuthMethod::OAuth,
            CredentialBackend::File,
        )
        .unwrap();
        assert_eq!(
            classification,
            ClaudeAuthClassification::OAuthMacosKeychainSharedLive
        );
    }

    #[test]
    fn classify_oauth_profile_as_scoped_keychain_when_supported() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(&user_home).unwrap();
        let profile_store = ProfileStore::new(dir.path());
        profile_store.create(Tool::Claude, "work").unwrap();
        let _platform_guard = EnvVarGuard::set("AISW_TEST_CLAUDE_PLATFORM", "macos");
        let _storage_guard = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _scheme_guard = EnvVarGuard::set("AISW_CLAUDE_KEYCHAIN_SCHEME", "scoped");
        let classification = classify_profile(
            &user_home,
            &profile_store,
            "work",
            AuthMethod::OAuth,
            CredentialBackend::File,
        )
        .unwrap();
        assert_eq!(
            classification,
            ClaudeAuthClassification::OAuthKeychainScopedByConfigDir
        );
    }

    #[test]
    fn classify_oauth_profile_as_file_backed_when_keychain_is_not_active() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(user_home.join(".claude")).unwrap();
        fs::write(
            user_home.join(".claude").join(".credentials.json"),
            br#"{"oauthToken":"tok"}"#,
        )
        .unwrap();
        let _storage_guard = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let classification = classify_profile(
            &user_home,
            &ProfileStore::new(dir.path()),
            "work",
            AuthMethod::OAuth,
            CredentialBackend::File,
        )
        .unwrap();
        assert_eq!(classification, ClaudeAuthClassification::OAuthFileBacked);
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

    #[test]
    fn system_keyring_api_key_add_cleans_secret_when_config_save_fails() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let keyring_dir = dir.path().join("keyring");
        let _guard = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", &keyring_dir);
        let (ps, cs) = stores(dir.path());
        cs.load().unwrap();
        fs::create_dir(dir.path().join("config.json.tmp")).unwrap();

        let err = add_api_key_with_backend(
            &ps,
            &cs,
            "work",
            valid_key(),
            None,
            CredentialBackend::SystemKeyring,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("config.json.tmp"),
            "unexpected error: {err:#}"
        );
        assert!(!ps.exists(Tool::Claude, "work"));
        assert!(secure_store::read_profile_secret(Tool::Claude, "work")
            .unwrap()
            .is_none());
        assert!(!cs
            .load()
            .unwrap()
            .profiles_for(Tool::Claude)
            .contains_key("work"));
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
            CredentialBackend::File,
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
    fn system_keyring_oauth_add_cleans_secret_when_config_save_fails() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let keyring_dir = dir.path().join("keyring");
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&bin_dir).unwrap();
        let _home = EnvVarGuard::set("HOME", &home);
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", &keyring_dir);
        let bin = make_oauth_mock(&bin_dir, true);

        let (ps, cs) = stores(dir.path());
        cs.load().unwrap();
        fs::create_dir(dir.path().join("config.json.tmp")).unwrap();

        let err = oauth::add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &bin,
            CredentialBackend::SystemKeyring,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("config.json.tmp"),
            "unexpected error: {err:#}"
        );
        assert!(!ps.exists(Tool::Claude, "work"));
        assert!(secure_store::read_profile_secret(Tool::Claude, "work")
            .unwrap()
            .is_none());
        assert!(!cs
            .load()
            .unwrap()
            .profiles_for(Tool::Claude)
            .contains_key("work"));
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
            CredentialBackend::File,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(err.to_string().contains("already exists as 'work'"));
        assert!(!ps.exists(Tool::Claude, "alias"));
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn oauth_duplicate_identity_allows_same_email_with_different_org() {
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
             printf '%s' '{\"oauthToken\":\"tok\",\"account\":{\"email\":\"burak@example.com\"}}' > \"$HOME/.claude/.credentials.json\"\n\
             printf '%s' '{\"oauthAccount\":{\"emailAddress\":\"burak@example.com\",\"organizationUuid\":\"org-b\"}}' > \"$HOME/.claude.json\"\n",
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
        ps.write_file(
            Tool::Claude,
            "work",
            OAUTH_ACCOUNT_FILE,
            br#"{"emailAddress":"burak@example.com","organizationUuid":"org-a"}"#,
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

        oauth::add_oauth_with(
            &ps,
            &cs,
            "alias",
            None,
            &bin,
            CredentialBackend::File,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(ps.exists(Tool::Claude, "alias"));
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
            CredentialBackend::File,
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
            CredentialBackend::File,
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
        let _scheme = EnvVarGuard::set("AISW_CLAUDE_KEYCHAIN_SCHEME", "shared");
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
            CredentialBackend::File,
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
        let _scheme = EnvVarGuard::set("AISW_CLAUDE_KEYCHAIN_SCHEME", "shared");
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
            CredentialBackend::File,
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
    fn oauth_on_non_macos_targets_profile_config_dir_during_login() {
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
             [ -n \"$CLAUDE_CONFIG_DIR\" ] || exit 7\n\
             printf '%s' \"$CLAUDE_CONFIG_DIR\" > \"$HOME/env_was_set\"\n\
             mkdir -p \"$CLAUDE_CONFIG_DIR\"\n\
             echo '{}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n\
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
            CredentialBackend::File,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(
            home.join("env_was_set").exists(),
            "CLAUDE_CONFIG_DIR should be set during non-macOS OAuth login"
        );
        assert_eq!(
            fs::read_to_string(home.join("env_was_set")).unwrap(),
            ps.profile_dir(Tool::Claude, "work").display().to_string()
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn oauth_on_macos_legacy_shared_keeps_live_keychain_target() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let _home = EnvVarGuard::set("HOME", &home);
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _platform = EnvVarGuard::set("AISW_TEST_CLAUDE_PLATFORM", "macos");
        let _scheme = EnvVarGuard::set("AISW_CLAUDE_KEYCHAIN_SCHEME", "shared");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));
        let _user = EnvVarGuard::set("USER", "tester");
        let bin = bin_dir.join("claude");
        let security_bin = bin_dir.join("security");
        write_security_mock(&security_bin);
        let _security = EnvVarGuard::set(
            "AISW_SECURITY_BIN",
            security_bin
                .to_str()
                .expect("security path should be utf-8"),
        );
        fs::write(
            &bin,
            "#!/bin/sh\n\
             [ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             [ -z \"$CLAUDE_CONFIG_DIR\" ] || { echo \"$CLAUDE_CONFIG_DIR\" > \"$HOME/env_was_set\"; exit 7; }\n\
             item=\"$AISW_KEYRING_TEST_DIR/Claude Code-credentials/${USER:-tester}\"\n\
             mkdir -p \"$item\"\n\
             printf '%s' \"${USER:-tester}\" > \"$item/account\"\n\
             printf '%s' '{\"account\":{\"email\":\"work@example.com\"}}' > \"$item/secret\"\n\
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
            CredentialBackend::File,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(
            !home.join("env_was_set").exists(),
            "CLAUDE_CONFIG_DIR should stay unset for legacy shared macOS OAuth login"
        );
        let stored = ps
            .read_file(Tool::Claude, "work", CREDENTIALS_FILE)
            .unwrap();
        assert_eq!(
            String::from_utf8(stored).unwrap().trim(),
            r#"{"account":{"email":"work@example.com"}}"#
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
             [ -n \"$CLAUDE_CONFIG_DIR\" ] || exit 7\n\
             printf '%s %s' \"$1\" \"$2\" > \"$HOME/login_args\"\n\
             mkdir -p \"$CLAUDE_CONFIG_DIR\"\n\
             echo '{}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n\
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
            CredentialBackend::File,
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
            CredentialBackend::File,
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
    fn oauth_on_macos_scoped_keychain_targets_profile_config_dir() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let _home = EnvVarGuard::set("HOME", &home);
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let _platform = EnvVarGuard::set("AISW_TEST_CLAUDE_PLATFORM", "macos");
        let _scheme = EnvVarGuard::set("AISW_CLAUDE_KEYCHAIN_SCHEME", "scoped");
        let bin = bin_dir.join("claude");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             [ -n \"$CLAUDE_CONFIG_DIR\" ] || exit 7\n\
             printf '%s %s' \"$1\" \"$2\" > \"$HOME/login_args\"\n\
             printf '%s' \"$CLAUDE_CONFIG_DIR\" > \"$HOME/env_was_set\"\n\
             mkdir -p \"$CLAUDE_CONFIG_DIR\"\n\
             echo '{}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n\
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
            CredentialBackend::File,
            std::time::Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(home.join("login_args")).unwrap(),
            "auth login"
        );
        assert_eq!(
            fs::read_to_string(home.join("env_was_set")).unwrap(),
            ps.profile_dir(Tool::Claude, "work").display().to_string()
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

        apply_live_credentials(
            &ps,
            "work",
            CredentialBackend::File,
            &user_home,
            StateMode::Shared,
        )
        .unwrap();

        let live_keychain = keychain::read_keychain_credentials().unwrap().unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&live_keychain).unwrap(),
            serde_json::json!({
                "claudeAiOauth": { "accessToken": "tok" },
                "mcpOAuth": { "x": { "clientId": "abc" } }
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

        apply_live_credentials(
            &ps,
            "work",
            CredentialBackend::SystemKeyring,
            &user_home,
            StateMode::Shared,
        )
        .unwrap();

        assert!(live_credentials_match(
            &ps,
            "work",
            CredentialBackend::SystemKeyring,
            &user_home,
            StateMode::Shared,
        )
        .unwrap());
    }

    #[test]
    fn keychain_apply_preserves_mcp_oauth_tokens() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        secure_store::write_profile_secret(
            Tool::Claude,
            "work",
            br#"{"claudeAiOauth":{"accessToken":"a"},"mcpOAuth":{"srv":{"clientId":"c"}}}"#,
        )
        .unwrap();

        apply_live_credentials(
            &ps,
            "work",
            CredentialBackend::SystemKeyring,
            &user_home,
            StateMode::Shared,
        )
        .unwrap();

        let live = keychain::read_keychain_credentials().unwrap().unwrap();
        let live_json: serde_json::Value = serde_json::from_slice(&live).unwrap();
        assert_eq!(live_json["claudeAiOauth"]["accessToken"], "a");
        assert_eq!(live_json["mcpOAuth"]["srv"]["clientId"], "c");

        assert!(live_credentials_match(
            &ps,
            "work",
            CredentialBackend::SystemKeyring,
            &user_home,
            StateMode::Shared,
        )
        .unwrap());
    }

    #[test]
    fn keychain_backed_profile_can_apply_when_live_storage_is_file() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(user_home.join(".claude")).unwrap();

        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keyring"));

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        secure_store::write_profile_secret(
            Tool::Claude,
            "work",
            br#"{"oauthToken":"tok","account":{"email":"work@example.com"}}"#,
        )
        .unwrap();

        apply_live_credentials(
            &ps,
            "work",
            CredentialBackend::SystemKeyring,
            &user_home,
            StateMode::Shared,
        )
        .unwrap();

        let live = fs::read_to_string(user_home.join(".claude").join(CREDENTIALS_FILE)).unwrap();
        let live_json: serde_json::Value = serde_json::from_str(&live).unwrap();
        assert_eq!(live_json["oauthToken"], "tok");
        assert_eq!(live_json["account"]["email"], "work@example.com");
    }

    #[test]
    fn normalize_credentials_bytes_decodes_nested_hex_wrapped_json() {
        let once = b"7b226f61757468546f6b656e223a22746f6b227d";
        let twice =
            b"37623232366636313735373436383534366636623635366532323361323237343666366232323764";

        let normalized_once = normalize_credentials_bytes(once).unwrap();
        let normalized_twice = normalize_credentials_bytes(twice).unwrap();

        assert_eq!(normalized_once, br#"{"oauthToken":"tok"}"#);
        assert_eq!(normalized_twice, br#"{"oauthToken":"tok"}"#);
    }

    #[test]
    fn keychain_live_credentials_match_decodes_hex_wrapped_live_payload() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keyring"));

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            CREDENTIALS_FILE,
            br#"{"oauthToken":"tok"}"#,
        )
        .unwrap();

        keychain::write_keychain_credentials(b"7b226f61757468546f6b656e223a22746f6b227d").unwrap();

        assert!(live_credentials_match(
            &ps,
            "work",
            CredentialBackend::File,
            &user_home,
            StateMode::Shared,
        )
        .unwrap());
    }

    #[test]
    fn identity_extraction_supports_all_claude_json_shapes() {
        // Format 1: account.email
        let json1 = br#"{"account":{"email":"user1@example.com"}}"#;
        assert_eq!(
            identity::resolve_identity_from_json_bytes(json1).unwrap(),
            Some("user1@example.com".to_owned())
        );

        // Format 2: oauthAccount.emailAddress (metadata file)
        let json2 = br#"{"oauthAccount":{"emailAddress":"user2@example.com"}}"#;
        assert_eq!(
            identity::resolve_identity_from_json_bytes(json2).unwrap(),
            Some("user2@example.com".to_owned())
        );

        // Format 3: top-level emailAddress
        let json3 = br#"{"emailAddress":"user3@example.com"}"#;
        assert_eq!(
            identity::resolve_identity_from_json_bytes(json3).unwrap(),
            Some("user3@example.com".to_owned())
        );

        // Format 4: invalid/missing
        let json4 = br#"{"something":"else"}"#;
        assert_eq!(
            identity::resolve_identity_from_json_bytes(json4).unwrap(),
            None
        );

        // Format 5: malformed JSON
        let json5 = br#"{"invalid": ...}"#;
        assert_eq!(
            identity::resolve_identity_from_json_bytes(json5).unwrap(),
            None
        );
    }
}
