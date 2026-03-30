use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;

use super::files;
use super::identity;
use super::secure_backend::{self, SecureBackend};
use super::secure_store;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::live_apply::LiveFileChange;
use crate::profile::ProfileStore;
use crate::types::{StateMode, Tool};

const AUTH_FILE: &str = "auth.json";
const CONFIG_FILE: &str = "config.toml";
const KEYCHAIN_SERVICE: &str = "Codex Auth";
const KEYCHAIN_BACKEND: SecureBackend = SecureBackend::SystemKeyring;

// Codex reads credentials from a file rather than the OS keyring when this is set.
const CONFIG_TOML_CONTENTS: &str = "cli_auth_credentials_store = \"file\"\n";

const OAUTH_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const OAUTH_CAPTURE_DIR: &str = ".oauth-capture";

fn live_dir(user_home: &Path) -> PathBuf {
    user_home.join(".codex")
}

fn live_auth_path(user_home: &Path) -> PathBuf {
    live_dir(user_home).join(AUTH_FILE)
}

fn live_config_path(user_home: &Path) -> PathBuf {
    live_dir(user_home).join(CONFIG_FILE)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveAuthStorage {
    Auto,
    File,
    Keyring,
    Unknown,
}

impl LiveAuthStorage {
    pub fn description(self) -> &'static str {
        match self {
            LiveAuthStorage::Auto => "auto",
            LiveAuthStorage::File => "file",
            LiveAuthStorage::Keyring => "keyring",
            LiveAuthStorage::Unknown => "unknown",
        }
    }
}

pub enum LiveCredentialSource {
    File(PathBuf),
    Keychain,
}

pub struct LiveCredentialSnapshot {
    pub bytes: Vec<u8>,
    pub source: LiveCredentialSource,
}

pub fn live_local_state_dir(user_home: &Path) -> Option<PathBuf> {
    let dir = live_dir(user_home);
    dir.exists().then_some(dir)
}

pub fn live_auth_storage(user_home: &Path) -> Result<Option<LiveAuthStorage>> {
    let Some(_) = live_local_state_dir(user_home) else {
        return Ok(None);
    };

    let config_path = live_config_path(user_home);
    if !config_path.exists() {
        return Ok(Some(LiveAuthStorage::Auto));
    }

    let contents = fs::read_to_string(&config_path)
        .with_context(|| format!("could not read {}", config_path.display()))?;
    Ok(Some(parse_live_auth_storage(&contents)))
}

pub fn live_credentials_snapshot_for_import(
    user_home: &Path,
) -> Result<Option<LiveCredentialSnapshot>> {
    let auth_path = live_auth_path(user_home);
    if !auth_path.exists() {
        if live_local_state_dir(user_home).is_none() {
            return Ok(None);
        }

        let Some(bytes) = read_live_keychain_credentials_for_import()? else {
            return Ok(None);
        };

        return Ok(Some(LiveCredentialSnapshot {
            bytes,
            source: LiveCredentialSource::Keychain,
        }));
    }

    let bytes =
        fs::read(&auth_path).with_context(|| format!("could not read {}", auth_path.display()))?;
    Ok(Some(LiveCredentialSnapshot {
        bytes,
        source: LiveCredentialSource::File(auth_path),
    }))
}

fn parse_live_auth_storage(contents: &str) -> LiveAuthStorage {
    let parsed = toml::from_str::<toml::Value>(contents).ok();
    if let Some(raw) = parsed
        .as_ref()
        .and_then(|value| value.get("cli_auth_credentials_store"))
        .and_then(|value| value.as_str())
    {
        return auth_storage_from_str(raw);
    }

    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "cli_auth_credentials_store" {
            continue;
        }
        return auth_storage_from_str(value.trim().trim_matches('"'));
    }

    LiveAuthStorage::Auto
}

fn auth_storage_from_str(raw: &str) -> LiveAuthStorage {
    match raw.trim().to_ascii_lowercase().as_str() {
        "auto" => LiveAuthStorage::Auto,
        "file" => LiveAuthStorage::File,
        "keyring" => LiveAuthStorage::Keyring,
        _ => LiveAuthStorage::Unknown,
    }
}

fn forced_auth_storage() -> Option<LiveAuthStorage> {
    match super::test_overrides::string("AISW_CODEX_AUTH_STORAGE").as_deref() {
        Some("auto") => Some(LiveAuthStorage::Auto),
        Some("file") => Some(LiveAuthStorage::File),
        Some("keychain") => Some(LiveAuthStorage::Keyring),
        _ => None,
    }
}

pub fn keychain_import_supported() -> bool {
    forced_auth_storage() == Some(LiveAuthStorage::Keyring) || super::system_keyring::is_available()
}

fn read_keychain_credentials() -> Result<Option<Vec<u8>>> {
    secure_backend::read_generic_password(KEYCHAIN_BACKEND, KEYCHAIN_SERVICE, None)
        .context("could not query the system keyring for Codex credentials")
}

fn live_keyring_account(credentials: &[u8]) -> Result<String> {
    let mut candidates = Vec::new();
    if let Some(identity) = super::identity::resolve_identity_from_json_bytes(credentials)? {
        candidates.push(identity);
    }

    secure_backend::find_generic_password_account_with_candidates(
        KEYCHAIN_BACKEND,
        KEYCHAIN_SERVICE,
        &candidates,
    )
    .context("could not determine the live Codex keyring account")?
    .ok_or_else(|| {
        anyhow::anyhow!(
            "could not determine the live Codex keyring account.\n  \
                 Sign in with Codex once on this machine so aisw can reuse the \
                 existing keyring entry, or switch Codex to file-backed auth."
        )
    })
}

pub fn read_live_keychain_credentials_for_import() -> Result<Option<Vec<u8>>> {
    if !keychain_import_supported() {
        return Ok(None);
    }

    match read_keychain_credentials() {
        Ok(credentials) => Ok(credentials),
        Err(err) if forced_auth_storage() != Some(LiveAuthStorage::Keyring) => {
            if err.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io_err| io_err.kind() == std::io::ErrorKind::NotFound)
            }) {
                Ok(None)
            } else {
                Err(err)
            }
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn write_file_store_config(profile_store: &ProfileStore, name: &str) -> Result<()> {
    profile_store.write_file(
        Tool::Codex,
        name,
        CONFIG_FILE,
        CONFIG_TOML_CONTENTS.as_bytes(),
    )
}

pub(crate) fn write_keyring_store_config(profile_store: &ProfileStore, name: &str) -> Result<()> {
    profile_store.write_file(
        Tool::Codex,
        name,
        CONFIG_FILE,
        b"cli_auth_credentials_store = \"keyring\"\n",
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

    files::cleanup_profile_on_error(
        write_file_store_config(profile_store, name),
        profile_store,
        Tool::Codex,
        name,
    )?;

    let auth_json = format!("{{\"token\":\"{}\"}}", key);
    files::cleanup_profile_on_error(
        profile_store.write_file(Tool::Codex, name, AUTH_FILE, auth_json.as_bytes()),
        profile_store,
        Tool::Codex,
        name,
    )?;

    config_store.add_profile(
        Tool::Codex,
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
            "Codex API key must not be empty.\n  \
             Get your API key at platform.openai.com → API Keys."
        );
    }
    Ok(())
}

/// Start the Codex OAuth flow using the installed `codex` binary.
///
/// On platforms where aisw has a native secure backend, Codex OAuth is captured
/// through a transient file-backed scratch dir and then persisted into the
/// secure backend. This avoids leaving `auth.json` in the managed profile while
/// also avoiding writes to the user's live Codex keyring item during `add`.
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
    let stored_backend = oauth_stored_backend();
    let capture_dir = oauth_capture_dir(&profile_dir);
    fs::create_dir_all(&capture_dir)
        .with_context(|| format!("could not create {}", capture_dir.display()))?;

    files::cleanup_profile_on_error(
        write_capture_file_store_config(&capture_dir),
        profile_store,
        Tool::Codex,
        name,
    )?;

    let auth_path = files::cleanup_profile_on_error(
        run_oauth_flow(codex_bin, &capture_dir, timeout, poll_interval),
        profile_store,
        Tool::Codex,
        name,
    )?;

    files::set_permissions_600(&auth_path)?;
    let auth_bytes =
        fs::read(&auth_path).with_context(|| format!("could not read {}", auth_path.display()))?;
    store_oauth_profile(
        profile_store,
        config_store,
        name,
        label,
        stored_backend,
        &auth_bytes,
    )
    .inspect_err(|_| {
        let _ = fs::remove_dir_all(&capture_dir);
    })?;
    let _ = fs::remove_dir_all(&capture_dir);

    Ok(())
}

fn store_oauth_profile(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    stored_backend: CredentialBackend,
    auth_bytes: &[u8],
) -> Result<()> {
    files::cleanup_profile_on_error(
        persist_oauth_storage(profile_store, name, stored_backend, auth_bytes),
        profile_store,
        Tool::Codex,
        name,
    )?;

    files::cleanup_profile_on_error(
        identity::ensure_unique_oauth_identity(
            profile_store,
            config_store,
            Tool::Codex,
            name,
            stored_backend,
        ),
        profile_store,
        Tool::Codex,
        name,
    )
    .inspect_err(|_| {
        if stored_backend == CredentialBackend::SystemKeyring {
            let _ = secure_store::delete_profile_secret(Tool::Codex, name);
        }
    })?;

    config_store
        .add_profile(
            Tool::Codex,
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
                let _ = secure_store::delete_profile_secret(Tool::Codex, name);
            }
            let _ = profile_store.delete(Tool::Codex, name);
        })?;

    Ok(())
}

fn persist_oauth_storage(
    profile_store: &ProfileStore,
    name: &str,
    stored_backend: CredentialBackend,
    auth_bytes: &[u8],
) -> Result<()> {
    match stored_backend {
        CredentialBackend::File => {
            write_file_store_config(profile_store, name)?;
            profile_store.write_file(Tool::Codex, name, AUTH_FILE, auth_bytes)
        }
        CredentialBackend::SystemKeyring => {
            write_keyring_store_config(profile_store, name)?;
            secure_store::write_profile_secret(Tool::Codex, name, auth_bytes)
        }
    }
}

fn oauth_stored_backend() -> CredentialBackend {
    match forced_auth_storage() {
        Some(LiveAuthStorage::File) => CredentialBackend::File,
        Some(LiveAuthStorage::Keyring) => CredentialBackend::SystemKeyring,
        Some(LiveAuthStorage::Auto | LiveAuthStorage::Unknown) | None => {
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

fn write_capture_file_store_config(capture_dir: &Path) -> Result<()> {
    let path = capture_dir.join(CONFIG_FILE);
    fs::write(&path, CONFIG_TOML_CONTENTS.as_bytes())
        .with_context(|| format!("could not write {}", path.display()))?;
    files::set_permissions_600(&path)
}

fn run_oauth_flow(
    codex_bin: &Path,
    capture_dir: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<PathBuf> {
    let mut child = Command::new(codex_bin)
        .arg("login")
        .env("CODEX_HOME", capture_dir)
        .spawn()
        .with_context(|| format!("could not spawn {}", codex_bin.display()))?;

    let auth_path = capture_dir.join(AUTH_FILE);
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

pub fn apply_live_files(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<()> {
    let live_dir = live_dir(user_home);
    std::fs::create_dir_all(&live_dir)
        .with_context(|| format!("could not create {}", live_dir.display()))?;

    let config_dest = live_config_path(user_home);
    match backend {
        CredentialBackend::File => {
            let auth_bytes = profile_store.read_file(Tool::Codex, name, AUTH_FILE)?;
            let auth_dest = live_auth_path(user_home);
            let config_bytes = desired_live_file_store_config(user_home)?.into_bytes();

            crate::live_apply::apply_transaction(vec![
                LiveFileChange::write(auth_dest, auth_bytes),
                LiveFileChange::write(config_dest, config_bytes),
            ])
        }
        CredentialBackend::SystemKeyring => {
            let bytes = secure_store::read_profile_secret(Tool::Codex, name)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "secure credentials for Codex CLI profile '{}' are missing from the system keyring",
                    name
                )
            })?;
            let account = live_keyring_account(&bytes)?;
            secure_backend::upsert_generic_password(
                KEYCHAIN_BACKEND,
                KEYCHAIN_SERVICE,
                &account,
                &bytes,
            )
            .context("could not write Codex credentials into the system keyring")?;
            crate::live_apply::apply_transaction(vec![LiveFileChange::write(
                config_dest,
                desired_live_keyring_store_config(user_home)?.into_bytes(),
            )])
        }
    }
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
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<bool> {
    let config_dest = live_config_path(user_home);
    if !config_dest.exists() {
        return Ok(false);
    }
    let config = std::fs::read_to_string(&config_dest)
        .with_context(|| format!("could not read {}", config_dest.display()))?;

    match backend {
        CredentialBackend::File => {
            if !files::stored_profile_file_matches_live(
                profile_store,
                Tool::Codex,
                name,
                AUTH_FILE,
                &live_auth_path(user_home),
            )? {
                return Ok(false);
            }
            Ok(config_uses_file_store(&config))
        }
        CredentialBackend::SystemKeyring => {
            let Some(live) = read_keychain_credentials()? else {
                return Ok(false);
            };
            let Some(stored) = secure_store::read_profile_secret(Tool::Codex, name)? else {
                return Ok(false);
            };
            Ok(live == stored && config_uses_keyring_store(&config))
        }
    }
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
    merge_store_config(current, "file")
}

fn merge_keyring_store_config(current: &str) -> String {
    merge_store_config(current, "keyring")
}

fn merge_store_config(current: &str, backend: &str) -> String {
    let mut replaced = false;
    let mut lines = Vec::new();
    for line in current.lines() {
        if line.trim_start().starts_with("cli_auth_credentials_store") {
            lines.push(format!("cli_auth_credentials_store = \"{}\"", backend));
            replaced = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !replaced {
        if !current.is_empty() && !current.ends_with('\n') {
            lines.push(String::new());
        }
        lines.push(format!("cli_auth_credentials_store = \"{}\"", backend));
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

fn config_uses_keyring_store(contents: &str) -> bool {
    contents
        .lines()
        .any(|line| line.trim() == "cli_auth_credentials_store = \"keyring\"")
}

fn desired_live_keyring_store_config(user_home: &Path) -> Result<String> {
    let config_dest = live_config_path(user_home);
    if config_dest.exists() {
        let current = std::fs::read_to_string(&config_dest)
            .with_context(|| format!("could not read {}", config_dest.display()))?;
        Ok(merge_keyring_store_config(&current))
    } else {
        Ok("cli_auth_credentials_store = \"keyring\"\n".to_owned())
    }
}

fn shell_single_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use tempfile::tempdir;

    use super::*;
    use crate::auth::secure_store;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;

    fn valid_key() -> &'static str {
        "sk-codex-test-key-12345"
    }

    fn stores(dir: &std::path::Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
    }

    fn write_security_mock(bin: &std::path::Path) {
        std::fs::write(
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
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
    fn parse_live_auth_storage_defaults_to_auto_when_missing() {
        assert_eq!(
            parse_live_auth_storage("model = \"gpt-5.4\"\n"),
            LiveAuthStorage::Auto
        );
    }

    #[test]
    fn parse_live_auth_storage_reads_keyring_backend() {
        assert_eq!(
            parse_live_auth_storage("cli_auth_credentials_store = \"keyring\"\n"),
            LiveAuthStorage::Keyring
        );
    }

    #[test]
    fn parse_live_auth_storage_handles_unknown_backend() {
        assert_eq!(
            parse_live_auth_storage("cli_auth_credentials_store = \"mystery\"\n"),
            LiveAuthStorage::Unknown
        );
    }

    #[test]
    fn live_credentials_snapshot_reads_auth_json() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();
        std::fs::write(
            user_home.join(".codex").join(AUTH_FILE),
            br#"{"token":"tok"}"#,
        )
        .unwrap();

        let snapshot = live_credentials_snapshot_for_import(&user_home)
            .unwrap()
            .expect("snapshot should exist");

        assert_eq!(snapshot.bytes, br#"{"token":"tok"}"#);
        match snapshot.source {
            LiveCredentialSource::File(path) => {
                assert_eq!(path, user_home.join(".codex").join(AUTH_FILE));
            }
            LiveCredentialSource::Keychain => panic!("expected file-backed snapshot"),
        }
    }

    #[test]
    fn live_credentials_snapshot_reads_keychain_when_enabled() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();
        std::fs::write(
            user_home.join(".codex").join(CONFIG_FILE),
            "model = \"gpt-5.4\"\n",
        )
        .unwrap();

        let _storage = EnvVarGuard::set("AISW_CODEX_AUTH_STORAGE", "keychain");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", user_home.join("keychain"));
        let item_dir = user_home
            .join("keychain")
            .join(KEYCHAIN_SERVICE)
            .join("tester");
        std::fs::create_dir_all(&item_dir).unwrap();
        std::fs::write(item_dir.join("account"), b"tester").unwrap();
        std::fs::write(item_dir.join("secret"), br#"{"token":"tok"}"#).unwrap();

        let snapshot = live_credentials_snapshot_for_import(&user_home)
            .unwrap()
            .expect("snapshot should exist");

        assert_eq!(snapshot.bytes, br#"{"token":"tok"}"#);
        match snapshot.source {
            LiveCredentialSource::Keychain => {}
            LiveCredentialSource::File(_) => panic!("expected keychain-backed snapshot"),
        }
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

        apply_live_files(&ps, "main", CredentialBackend::File, &user_home).unwrap();

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
        let _storage = EnvVarGuard::set("AISW_CODEX_AUTH_STORAGE", "file");
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
             [ -f \"$CODEX_HOME/config.toml\" ] && touch \"$CODEX_HOME/../config_was_present\"\n\
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
        let _storage = EnvVarGuard::set("AISW_CODEX_AUTH_STORAGE", "file");
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
        let _storage = EnvVarGuard::set("AISW_CODEX_AUTH_STORAGE", "file");
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
        let _storage = EnvVarGuard::set("AISW_CODEX_AUTH_STORAGE", "file");
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
        let _storage = EnvVarGuard::set("AISW_CODEX_AUTH_STORAGE", "file");
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
        assert!(!ps.exists(Tool::Codex, "alias"));
    }

    #[test]
    fn keychain_backed_profile_applies_and_matches_live_keychain() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(&user_home).unwrap();

        let security_bin = bin_dir.join("security");
        write_security_mock(&security_bin);

        let _storage = EnvVarGuard::set("AISW_CODEX_AUTH_STORAGE", "keychain");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));
        let _security = EnvVarGuard::set(
            "AISW_SECURITY_BIN",
            security_bin
                .to_str()
                .expect("security path should be utf-8"),
        );

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Codex, "work").unwrap();
        write_keyring_store_config(&ps, "work").unwrap();
        secure_store::write_profile_secret(Tool::Codex, "work", br#"{"token":"tok"}"#).unwrap();
        secure_backend::upsert_generic_password(
            KEYCHAIN_BACKEND,
            KEYCHAIN_SERVICE,
            "tester",
            br#"{"token":"old"}"#,
        )
        .unwrap();

        apply_live_files(&ps, "work", CredentialBackend::SystemKeyring, &user_home).unwrap();

        assert!(
            live_files_match(&ps, "work", CredentialBackend::SystemKeyring, &user_home).unwrap()
        );
    }

    #[test]
    fn keychain_backed_profile_errors_without_live_keyring_account() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(&user_home).unwrap();

        let _storage = EnvVarGuard::set("AISW_CODEX_AUTH_STORAGE", "keychain");
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));

        let (ps, _cs) = stores(dir.path());
        ps.create(Tool::Codex, "work").unwrap();
        write_keyring_store_config(&ps, "work").unwrap();
        secure_store::write_profile_secret(Tool::Codex, "work", br#"{"token":"tok"}"#).unwrap();

        let err = apply_live_files(&ps, "work", CredentialBackend::SystemKeyring, &user_home)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("could not determine the live Codex keyring account"));
    }

    #[test]
    fn keychain_backed_profile_prefers_identity_named_live_account() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let aisw_home = dir.path().join("aisw");
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();
        std::fs::create_dir_all(&aisw_home).unwrap();
        let _home = EnvVarGuard::set("HOME", &user_home);
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path().join("keychain"));
        let profiles_home = aisw_home.join("profiles");
        let ps = ProfileStore::new(&profiles_home);

        ps.create(Tool::Codex, "work").unwrap();
        write_keyring_store_config(&ps, "work").unwrap();
        secure_store::write_profile_secret(
            Tool::Codex,
            "work",
            br#"{"token":"new","email":"work@example.com"}"#,
        )
        .unwrap();
        secure_backend::upsert_generic_password(
            KEYCHAIN_BACKEND,
            KEYCHAIN_SERVICE,
            "a-stale",
            br#"{"token":"stale"}"#,
        )
        .unwrap();
        secure_backend::upsert_generic_password(
            KEYCHAIN_BACKEND,
            KEYCHAIN_SERVICE,
            "work@example.com",
            br#"{"token":"old"}"#,
        )
        .unwrap();

        apply_live_files(&ps, "work", CredentialBackend::SystemKeyring, &user_home).unwrap();

        assert_eq!(
            secure_backend::read_generic_password(
                KEYCHAIN_BACKEND,
                KEYCHAIN_SERVICE,
                Some("work@example.com"),
            )
            .unwrap(),
            Some(br#"{"token":"new","email":"work@example.com"}"#.to_vec())
        );
        assert_eq!(
            secure_backend::read_generic_password(
                KEYCHAIN_BACKEND,
                KEYCHAIN_SERVICE,
                Some("a-stale"),
            )
            .unwrap(),
            Some(br#"{"token":"stale"}"#.to_vec())
        );
    }
}
