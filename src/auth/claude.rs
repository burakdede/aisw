use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::fs;

use super::files;
use super::identity;
use super::secure_backend::{self, SecureBackend};
use super::secure_store;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::output;
use crate::profile::ProfileStore;
use crate::tool_detection;
use crate::types::{StateMode, Tool};

const CREDENTIALS_FILE: &str = ".credentials.json";
const OAUTH_ACCOUNT_FILE: &str = "oauth-account.json";
const OAUTH_CAPTURE_DIR: &str = ".oauth-capture";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const KEYCHAIN_BACKEND: SecureBackend = SecureBackend::SystemKeyring;

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

fn live_account_metadata_path(user_home: &Path) -> PathBuf {
    user_home.join(".claude.json")
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

    // On macOS, Claude Code reads credentials from the Keychain even when a
    // credentials file is also present on disk. Prefer Keychain here so that
    // apply_live_credentials updates what Claude actually reads, not just the
    // file that Claude ignores.
    #[cfg(target_os = "macos")]
    if super::system_keyring::is_available() && read_keychain_credentials().ok().flatten().is_some()
    {
        return ClaudeAuthStorage::Keychain;
    }

    if live_credentials_path(user_home).exists() {
        ClaudeAuthStorage::File
    } else if super::system_keyring::is_available()
        && read_keychain_credentials().ok().flatten().is_some()
    {
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
    forced_auth_storage() == Some(ClaudeAuthStorage::Keychain)
        || super::system_keyring::is_available()
}

fn watch_keychain_during_oauth() -> bool {
    match forced_auth_storage() {
        Some(ClaudeAuthStorage::File) => false,
        Some(ClaudeAuthStorage::Keychain) => true,
        None => super::system_keyring::is_available(),
    }
}

fn keychain_account() -> String {
    secure_backend::find_generic_password_account(KEYCHAIN_BACKEND, KEYCHAIN_SERVICE)
        .ok()
        .flatten()
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "aisw".to_owned())
}

fn read_keychain_credentials() -> Result<Option<Vec<u8>>> {
    secure_backend::read_generic_password(KEYCHAIN_BACKEND, KEYCHAIN_SERVICE, None)
        .context("could not query the system keyring for Claude Code credentials")
}

pub fn read_live_keychain_credentials_for_import() -> Result<Option<Vec<u8>>> {
    if forced_auth_storage() == Some(ClaudeAuthStorage::File) {
        return Ok(None);
    }
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
    let local_state = live_local_state_dir(user_home);

    if cfg!(target_os = "macos") {
        if local_state.is_some() {
            if let Some(bytes) = read_live_keychain_credentials_for_import()? {
                return Ok(Some(LiveCredentialSnapshot {
                    bytes,
                    source: LiveCredentialSource::Keychain,
                }));
            }
        }

        if live_path.exists() {
            let bytes = std::fs::read(&live_path)
                .with_context(|| format!("could not read {}", live_path.display()))?;
            return Ok(Some(LiveCredentialSnapshot {
                bytes,
                source: LiveCredentialSource::File(live_path),
            }));
        }

        return Ok(None);
    }

    if live_path.exists() {
        let bytes = std::fs::read(&live_path)
            .with_context(|| format!("could not read {}", live_path.display()))?;
        return Ok(Some(LiveCredentialSnapshot {
            bytes,
            source: LiveCredentialSource::File(live_path),
        }));
    }

    if local_state.is_none() {
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
    if cfg!(target_os = "macos") && super::test_overrides::var("AISW_KEYRING_TEST_DIR").is_none() {
        return super::macos_keychain::upsert_generic_password(
            KEYCHAIN_SERVICE,
            &keychain_account(),
            bytes,
            &trusted_claude_app_paths(),
        )
        .context("could not write Claude Code credentials into the system keyring");
    }

    secure_backend::upsert_generic_password(
        KEYCHAIN_BACKEND,
        KEYCHAIN_SERVICE,
        &keychain_account(),
        bytes,
    )
    .context("could not write Claude Code credentials into the system keyring")
}

fn trusted_claude_app_paths() -> Vec<PathBuf> {
    tool_detection::detect(Tool::Claude)
        .map(|detected| vec![detected.binary_path])
        .unwrap_or_default()
}

fn oauth_stored_backend() -> CredentialBackend {
    if cfg!(target_os = "macos") {
        return CredentialBackend::File;
    }

    match forced_auth_storage() {
        Some(ClaudeAuthStorage::File) => CredentialBackend::File,
        Some(ClaudeAuthStorage::Keychain) => CredentialBackend::SystemKeyring,
        None => {
            if super::system_keyring::is_available() {
                CredentialBackend::SystemKeyring
            } else {
                CredentialBackend::File
            }
        }
    }
}

fn oauth_capture_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(OAUTH_CAPTURE_DIR)
}

fn persist_oauth_storage(
    profile_store: &ProfileStore,
    name: &str,
    stored_backend: CredentialBackend,
    auth_bytes: &[u8],
) -> Result<()> {
    match stored_backend {
        CredentialBackend::File => {
            profile_store.write_file(Tool::Claude, name, CREDENTIALS_FILE, auth_bytes)
        }
        CredentialBackend::SystemKeyring => {
            secure_store::write_profile_secret(Tool::Claude, name, auth_bytes)
        }
    }
}

fn live_keychain_payload(credentials: &[u8]) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(credentials) else {
        return credentials.to_vec();
    };
    let Some(claude_ai_oauth) = value.get("claudeAiOauth") else {
        return credentials.to_vec();
    };
    serde_json::to_vec(&serde_json::json!({ "claudeAiOauth": claude_ai_oauth }))
        .unwrap_or_else(|_| credentials.to_vec())
}

fn read_live_oauth_account_metadata(user_home: &Path) -> Result<Option<Vec<u8>>> {
    let path = live_account_metadata_path(user_home);
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read(&path).with_context(|| format!("could not read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&contents)
        .with_context(|| format!("could not parse {}", path.display()))?;
    let Some(oauth_account) = value.get("oauthAccount") else {
        return Ok(None);
    };

    serde_json::to_vec(oauth_account)
        .map(Some)
        .context("could not serialize Claude oauthAccount metadata")
}

fn persist_live_oauth_account_metadata(
    profile_store: &ProfileStore,
    name: &str,
    user_home: &Path,
) -> Result<()> {
    let Some(metadata) = read_live_oauth_account_metadata(user_home)? else {
        return Ok(());
    };
    profile_store.write_file(Tool::Claude, name, OAUTH_ACCOUNT_FILE, &metadata)
}

pub fn capture_live_oauth_account_metadata(
    profile_store: &ProfileStore,
    name: &str,
    user_home: &Path,
) -> Result<()> {
    persist_live_oauth_account_metadata(profile_store, name, user_home)
}

fn apply_live_oauth_account_metadata(
    profile_store: &ProfileStore,
    name: &str,
    user_home: &Path,
) -> Result<()> {
    let profile_path = profile_store
        .profile_dir(Tool::Claude, name)
        .join(OAUTH_ACCOUNT_FILE);
    if !profile_path.exists() {
        return Ok(());
    }

    let oauth_account = profile_store.read_file(Tool::Claude, name, OAUTH_ACCOUNT_FILE)?;
    let oauth_account_value: serde_json::Value = serde_json::from_slice(&oauth_account)
        .with_context(|| format!("could not parse {}", profile_path.display()))?;

    let live_path = live_account_metadata_path(user_home);
    let mut live_json = if live_path.exists() {
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(&live_path)
                .with_context(|| format!("could not read {}", live_path.display()))?,
        )
        .with_context(|| format!("could not parse {}", live_path.display()))?
    } else {
        serde_json::json!({})
    };

    if !live_json.is_object() {
        live_json = serde_json::json!({});
    }

    if let Some(obj) = live_json.as_object_mut() {
        obj.insert("oauthAccount".to_owned(), oauth_account_value);
    }

    let bytes = serde_json::to_vec_pretty(&live_json)
        .context("could not serialize Claude metadata for live state")?;
    fs::write(&live_path, bytes)
        .with_context(|| format!("could not write {}", live_path.display()))?;
    files::set_permissions_600(&live_path)
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

    let credentials = serde_json::to_string(&serde_json::json!({ "apiKey": key }))
        .context("could not serialize API key credentials")?;
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
    let stored_backend = oauth_stored_backend();
    let capture_dir = oauth_capture_dir(&profile_dir);
    fs::create_dir_all(&capture_dir)
        .with_context(|| format!("could not create {}", capture_dir.display()))?;

    let auth_bytes = files::cleanup_profile_on_error(
        run_oauth_flow(claude_bin, &capture_dir, timeout, poll_interval),
        profile_store,
        Tool::Claude,
        name,
    )?;

    files::cleanup_profile_on_error(
        persist_oauth_storage(profile_store, name, stored_backend, &auth_bytes),
        profile_store,
        Tool::Claude,
        name,
    )
    .inspect_err(|_| {
        let _ = fs::remove_dir_all(&capture_dir);
    })?;

    if let Some(user_home) = dirs::home_dir() {
        files::cleanup_profile_on_error(
            persist_live_oauth_account_metadata(profile_store, name, &user_home),
            profile_store,
            Tool::Claude,
            name,
        )
        .inspect_err(|_| {
            let _ = fs::remove_dir_all(&capture_dir);
        })?;
    }

    files::cleanup_profile_on_error(
        identity::ensure_unique_oauth_identity(
            profile_store,
            config_store,
            Tool::Claude,
            name,
            stored_backend,
        ),
        profile_store,
        Tool::Claude,
        name,
    )
    .inspect_err(|_| {
        if stored_backend == CredentialBackend::SystemKeyring {
            let _ = secure_store::delete_profile_secret(Tool::Claude, name);
        }
    })?;

    config_store
        .add_profile(
            Tool::Claude,
            name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: stored_backend,
                label,
            },
        )
        .inspect_err(|_| {
            if stored_backend == CredentialBackend::SystemKeyring {
                let _ = secure_store::delete_profile_secret(Tool::Claude, name);
            }
            let _ = profile_store.delete(Tool::Claude, name);
        })?;

    let _ = fs::remove_dir_all(&capture_dir);

    Ok(())
}

fn run_oauth_flow(
    claude_bin: &Path,
    capture_dir: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<Vec<u8>> {
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

    output::print_info("Claude sign-in will continue in your browser.");

    // When the keychain watcher is active we let Claude write credentials to its
    // normal location (real config dir + macOS Keychain). This avoids the
    // `CLAUDE_CONFIG_DIR` override that causes Claude to fall back to the
    // remote-callback / authentication-code flow (`code=true`).  The keychain
    // watcher detects the new credential without needing a capture dir.
    //
    // When the keychain watcher is unavailable (non-macOS or keyring not
    // available) we keep the capture-dir approach so the file poller can pick
    // up credentials written to `CLAUDE_CONFIG_DIR`.
    let mut cmd = Command::new(claude_bin);
    cmd.arg("auth").arg("login");
    if !watch_keychain {
        cmd.env("CLAUDE_CONFIG_DIR", capture_dir);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("could not spawn {}", claude_bin.display()))?;

    let credentials_path = capture_dir.join(CREDENTIALS_FILE);
    let deadline = Instant::now() + timeout;

    loop {
        if credentials_path.exists() {
            // Give the binary a moment to finish writing, then kill it if still running.
            let _ = child.kill();
            let _ = child.wait();
            let bytes = fs::read(&credentials_path)
                .with_context(|| format!("could not read {}", credentials_path.display()))?;
            files::set_permissions_600(&credentials_path)?;
            return Ok(bytes);
        }

        if watch_keychain {
            if let Some(current) = read_keychain_credentials()? {
                let changed = keychain_before.as_deref() != Some(current.as_slice());
                if changed {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(current);
                }
            }
        }

        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("could not poll {}", claude_bin.display()))?
        {
            if watch_keychain {
                if let Some(current) = read_keychain_credentials()? {
                    if status.success() || keychain_before.is_none() {
                        return Ok(current);
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
            write_keychain_credentials(&live_keychain_payload(&stored))
        }
    }?;

    apply_live_oauth_account_metadata(profile_store, name, user_home)
}

pub fn imported_profile_backend(source: &LiveCredentialSource) -> CredentialBackend {
    if cfg!(target_os = "macos") && matches!(source, LiveCredentialSource::Keychain) {
        CredentialBackend::File
    } else {
        match source {
            LiveCredentialSource::File(_) => CredentialBackend::File,
            LiveCredentialSource::Keychain => CredentialBackend::SystemKeyring,
        }
    }
}

pub fn uses_live_keychain(user_home: &Path) -> bool {
    cfg!(target_os = "macos") && matches!(auth_storage(user_home), ClaudeAuthStorage::Keychain)
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
            let Some(live) = read_keychain_credentials()? else {
                return Ok(false);
            };
            // The Keychain only stores the claudeAiOauth subset (written by
            // live_keychain_payload). Compare as parsed JSON values to handle
            // the trailing newline added by the security CLI and key ordering.
            let live_value = serde_json::from_slice::<serde_json::Value>(&live)
                .context("could not parse live Keychain credentials")?;
            let stored_payload = live_keychain_payload(&stored);
            let stored_value = serde_json::from_slice::<serde_json::Value>(&stored_payload)
                .context("could not parse stored credential payload")?;
            Ok(live_value == stored_value)
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

    use crate::auth::test_overrides::EnvVarGuard;

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
            "[ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             mkdir -p \"$CLAUDE_CONFIG_DIR\"\n\
             echo '{\"oauthToken\":\"tok\"}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n\
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
             echo '{\"oauthToken\":\"tok\",\"account\":{\"email\":\"burak@example.com\"}}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n",
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
        add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &claude_bin,
            Duration::from_secs(2),
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
    #[cfg(unix)]
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
        write_keychain_credentials(existing).unwrap();

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "work",
            None,
            &claude_bin,
            Duration::from_secs(2),
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
             [ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             mkdir -p \"$CLAUDE_CONFIG_DIR\"\n\
             echo \"$CLAUDE_CONFIG_DIR\" > \"$(dirname \"$CLAUDE_CONFIG_DIR\")/env_was_set\"\n\
             echo '{}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n\
             exit 0\n",
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
    #[cfg(unix)]
    fn oauth_uses_auth_login_subcommand() {
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
             mkdir -p \"$CLAUDE_CONFIG_DIR\"\n\
             printf '%s %s' \"$1\" \"$2\" > \"$(dirname \"$CLAUDE_CONFIG_DIR\")/login_args\"\n\
             echo '{}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n\
             exit 0\n",
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

        let sentinel = ps.profile_dir(Tool::Claude, "work").join("login_args");
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "auth login");
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

        let live_keychain = read_keychain_credentials().unwrap().unwrap();
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
