use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;

use super::files;
use super::identity;
use super::secure_store;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::live_apply::LiveFileChange;
use crate::profile::ProfileStore;
use crate::types::Tool;

pub(crate) const KEYRING_METADATA_FILE: &str = "keyring.json";
const SECRET_FILE: &str = "keyring-secret.json";
const API_KEY_FILE: &str = "api-key.json";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(180);
const KEYRING_SERVICE: &str = "gemini";
const KEYRING_ACCOUNT: &str = "antigravity";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityAuthClassification {
    OauthSharedLiveKeyring,
    ApiKeyEnvironment,
}

impl AntigravityAuthClassification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OauthSharedLiveKeyring => "oauth_shared_live_keyring",
            Self::ApiKeyEnvironment => "api_key_environment",
        }
    }

    pub fn human_label(self) -> &'static str {
        match self {
            Self::OauthSharedLiveKeyring => "OAuth shared live keyring",
            Self::ApiKeyEnvironment => "API key via GEMINI_API_KEY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyringRef {
    pub service: String,
    pub account: String,
}

#[derive(Debug, Clone)]
pub struct LiveSnapshot {
    pub keyring_ref: KeyringRef,
    pub keyring_secret: Option<Vec<u8>>,
    pub settings: Option<Vec<u8>>,
}

pub fn live_app_dir(user_home: &Path) -> PathBuf {
    user_home.join(".gemini").join("antigravity-cli")
}

pub fn live_shared_dir(user_home: &Path) -> PathBuf {
    user_home.join(".gemini").join("config")
}

pub fn default_live_keyring_ref() -> KeyringRef {
    KeyringRef {
        service: KEYRING_SERVICE.to_owned(),
        account: KEYRING_ACCOUNT.to_owned(),
    }
}

pub fn classify_profile(
    _profile_store: &ProfileStore,
    _name: &str,
    auth_method: AuthMethod,
    _credential_backend: CredentialBackend,
) -> Result<AntigravityAuthClassification> {
    if auth_method == AuthMethod::OAuth {
        return Ok(AntigravityAuthClassification::OauthSharedLiveKeyring);
    }
    Ok(AntigravityAuthClassification::ApiKeyEnvironment)
}

pub fn add_api_key_with_backend(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    profile_name: &str,
    key: &str,
    label: Option<String>,
    backend: CredentialBackend,
) -> Result<()> {
    crate::auth::validate_api_key_charset(
        key,
        "Antigravity",
        "Get your Gemini API key at aistudio.google.com → Get API Key.",
    )?;

    if let Some(existing_name) = identity::existing_api_key_profile_for_secret(
        profile_store,
        config_store,
        Tool::Antigravity,
        key,
    )? {
        bail!(
            "Antigravity API key already exists as profile '{}'.\n  \
             Use that profile or provide a different API key.",
            existing_name
        );
    }

    profile_store.create(Tool::Antigravity, profile_name)?;
    let result = (|| {
        let credentials = serde_json::to_vec(&serde_json::json!({ "apiKey": key }))
            .context("could not serialize Antigravity API key credentials")?;
        match backend {
            CredentialBackend::File => profile_store.write_file(
                Tool::Antigravity,
                profile_name,
                API_KEY_FILE,
                &credentials,
            )?,
            CredentialBackend::SystemKeyring => {
                secure_store::write_profile_secret(Tool::Antigravity, profile_name, &credentials)?;
            }
        }
        config_store.add_profile(
            Tool::Antigravity,
            profile_name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::ApiKey,
                credential_backend: backend,
                label,
            },
        )?;
        Ok::<(), anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = profile_store.delete(Tool::Antigravity, profile_name);
        if backend == CredentialBackend::SystemKeyring {
            let _ = secure_store::delete_profile_secret(Tool::Antigravity, profile_name);
        }
    }
    result
}

pub fn read_api_key(
    profile_store: &ProfileStore,
    profile_name: &str,
    backend: CredentialBackend,
) -> Result<String> {
    let bytes = match backend {
        CredentialBackend::File => {
            profile_store.read_file(Tool::Antigravity, profile_name, API_KEY_FILE)?
        }
        CredentialBackend::SystemKeyring => {
            secure_store::read_profile_secret(Tool::Antigravity, profile_name)?
                .context("managed Antigravity API key is missing")?
        }
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("could not parse managed Antigravity API key")?;
    value
        .get("apiKey")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .context("managed Antigravity API key is missing apiKey field")
}

/// Reads the managed keyring secret for a profile.
///
/// Normalize legacy output framing on read and persist, so switching also
/// repairs profiles captured with a newline or security CLI hex wrapping.
/// The upstream envelope remains intact; decoding for identity is separate.
pub fn read_managed_secret(
    profile_store: &ProfileStore,
    profile_name: &str,
    backend: CredentialBackend,
) -> Result<Option<Vec<u8>>> {
    let secret = match backend {
        CredentialBackend::File => {
            let path = profile_store
                .validated_profile_dir(Tool::Antigravity, profile_name)?
                .join(SECRET_FILE);
            if !path.exists() {
                return Ok(None);
            }
            profile_store
                .read_file(Tool::Antigravity, profile_name, SECRET_FILE)
                .map(Some)
        }
        CredentialBackend::SystemKeyring => {
            secure_store::read_profile_secret(Tool::Antigravity, profile_name)
        }
    }?;

    secret.map(|bytes| normalize_secret(&bytes)).transpose()
}

pub fn persist_managed_secret(
    profile_store: &ProfileStore,
    profile_name: &str,
    backend: CredentialBackend,
    secret: &[u8],
) -> Result<()> {
    let secret = normalize_secret(secret)?;
    match backend {
        CredentialBackend::File => {
            profile_store.write_file(Tool::Antigravity, profile_name, SECRET_FILE, &secret)
        }
        CredentialBackend::SystemKeyring => {
            secure_store::write_profile_secret(Tool::Antigravity, profile_name, &secret)
        }
    }
}

pub fn live_credentials_snapshot_for_import(user_home: &Path) -> Result<Option<LiveSnapshot>> {
    let snapshot = capture_live_snapshot(user_home)?;
    if snapshot.keyring_secret.is_none() {
        return Ok(None);
    }
    Ok(Some(snapshot))
}

pub fn capture_live_snapshot(user_home: &Path) -> Result<LiveSnapshot> {
    let keyring_ref = default_live_keyring_ref();
    Ok(LiveSnapshot {
        keyring_secret: super::system_keyring::read_go_secret(
            &keyring_ref.service,
            &keyring_ref.account,
        )?
        .map(|bytes| normalize_secret(&bytes))
        .transpose()?,
        keyring_ref,
        settings: read_settings_file(user_home)?,
    })
}

fn read_settings_file(user_home: &Path) -> Result<Option<Vec<u8>>> {
    let path = live_app_dir(user_home).join("settings.json");
    crate::live_apply::validate_live_path(user_home, &path)?;
    if !path.exists() {
        return Ok(None);
    }
    fs::read(&path)
        .map(Some)
        .with_context(|| format!("could not read {}", path.display()))
}

pub(crate) fn uses_oauth_provider(settings: Option<&[u8]>) -> Result<bool> {
    let Some(bytes) = settings else {
        return Ok(true);
    };
    let value: serde_json::Value =
        serde_json::from_slice(bytes).context("could not parse Antigravity settings")?;
    let object = value
        .as_object()
        .context("Antigravity settings must be a JSON object")?;
    Ok(object
        .get("modelProvider")
        .and_then(serde_json::Value::as_str)
        != Some("gemini"))
}

/// Preserve the upstream envelope while repairing recognizable legacy CLI hex output.
fn normalize_secret(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut candidate = super::trim_credential_payload(bytes).to_vec();
    for _ in 0..=3 {
        if decode_secret_value(&candidate)?.is_some() {
            if let Some(json) = super::normalize_keychain_json(&candidate) {
                return Ok(json);
            }
            return Ok(candidate);
        }
        let Some(decoded) = decode_hex(&candidate) else {
            break;
        };
        candidate = super::trim_credential_payload(&decoded).to_vec();
    }
    Ok(super::trim_credential_payload(bytes).to_vec())
}

fn live_secret_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    let normalized = normalize_secret(bytes)?;
    if cfg!(target_os = "macos") || !normalized.starts_with(b"go-keyring-") {
        return Ok(normalized);
    }
    let value = decode_secret_value(&normalized)?
        .context("Go keyring envelope must contain credential JSON")?;
    serde_json::to_vec(&value).context("could not serialize Go credential JSON")
}

fn decode_hex(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() || bytes.len() % 2 != 0 {
        return None;
    }
    bytes
        .chunks_exact(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            Some(((high << 4) | low) as u8)
        })
        .collect()
}

fn decode_secret_value(bytes: &[u8]) -> Result<Option<serde_json::Value>> {
    let decoded;
    let json = if let Some(encoded) = bytes.strip_prefix(b"go-keyring-base64:") {
        decoded = STANDARD
            .decode(encoded)
            .context("invalid Antigravity keyring base64 envelope")?;
        decoded.as_slice()
    } else if let Some(encoded) = bytes.strip_prefix(b"go-keyring-encoded:") {
        decoded = decode_hex(encoded).context("invalid Antigravity keyring hex envelope")?;
        decoded.as_slice()
    } else {
        bytes
    };
    let value = serde_json::from_slice::<serde_json::Value>(json).ok();
    if bytes.starts_with(b"go-keyring-")
        && !value.as_ref().is_some_and(serde_json::Value::is_object)
    {
        bail!("Antigravity keyring envelope does not contain a JSON credential object");
    }
    Ok(value.filter(serde_json::Value::is_object))
}

pub(crate) fn credential_json_value(bytes: &[u8]) -> Result<Option<serde_json::Value>> {
    decode_secret_value(&normalize_secret(bytes)?)
}

pub fn write_profile_snapshot(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    profile_name: &str,
    label: Option<String>,
    backend: CredentialBackend,
    snapshot: &LiveSnapshot,
    overwrite_existing: bool,
) -> Result<()> {
    let existing_secret = if overwrite_existing {
        read_managed_secret(profile_store, profile_name, backend)?
    } else {
        None
    };
    let result = write_profile_snapshot_inner(
        profile_store,
        config_store,
        profile_name,
        label,
        backend,
        snapshot,
        overwrite_existing,
    );
    if result.is_err() && overwrite_existing && backend == CredentialBackend::SystemKeyring {
        match existing_secret {
            Some(secret) => {
                let _ =
                    secure_store::write_profile_secret(Tool::Antigravity, profile_name, &secret);
            }
            None => {
                let _ = secure_store::delete_profile_secret(Tool::Antigravity, profile_name);
            }
        }
    }
    result
}

fn write_profile_snapshot_inner(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    profile_name: &str,
    label: Option<String>,
    backend: CredentialBackend,
    snapshot: &LiveSnapshot,
    overwrite_existing: bool,
) -> Result<()> {
    if snapshot.keyring_secret.is_none() {
        bail!("no Antigravity keyring credential found. Sign in with 'agy' first, then retry.");
    }

    if !uses_oauth_provider(snapshot.settings.as_deref())? {
        bail!("Antigravity is using an API-key provider. Remove modelProvider and sign in with agy before capturing an OAuth profile.");
    }

    if let Some(existing) = identity::existing_antigravity_oauth_profile_for_live_secret(
        profile_store,
        config_store,
        snapshot.keyring_secret.as_deref(),
    )? {
        if existing != profile_name {
            bail!(
                "An Antigravity OAuth profile for this account already exists as '{}'.\n  \
                 Use that profile or remove it before saving another alias.",
                existing
            );
        }
    }

    persist_profile_keyring_ref(profile_store, profile_name, &snapshot.keyring_ref)?;
    if let Some(secret) = snapshot.keyring_secret.as_deref() {
        persist_managed_secret(profile_store, profile_name, backend, secret)?;
    }

    identity::ensure_unique_oauth_identity(
        profile_store,
        config_store,
        Tool::Antigravity,
        profile_name,
        backend,
    )?;

    let meta = ProfileMeta {
        added_at: Utc::now(),
        auth_method: AuthMethod::OAuth,
        credential_backend: backend,
        label,
    };
    if overwrite_existing {
        config_store.upsert_profile(Tool::Antigravity, profile_name, meta)?;
    } else {
        config_store.add_profile(Tool::Antigravity, profile_name, meta)?;
    }
    Ok(())
}

pub fn apply_live_credentials(
    profile_store: &ProfileStore,
    profile_name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<()> {
    let keyring_ref = read_profile_keyring_ref(profile_store, profile_name)?;
    let Some(secret) = read_managed_secret(profile_store, profile_name, backend)? else {
        bail!(
            "managed Antigravity credential is missing for profile '{}'",
            profile_name
        );
    };

    apply_provider_settings(user_home, None)?;
    super::system_keyring::write_go_secret(
        &keyring_ref.service,
        &keyring_ref.account,
        &live_secret_bytes(&secret)?,
    )
}

pub fn apply_api_key_settings(user_home: &Path) -> Result<()> {
    apply_provider_settings(user_home, Some("gemini"))
}

fn apply_provider_settings(user_home: &Path, provider: Option<&str>) -> Result<()> {
    let path = live_app_dir(user_home).join("settings.json");
    let mut settings = if let Some(bytes) = read_settings_file(user_home)? {
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .with_context(|| format!("could not parse {}", path.display()))?
    } else {
        serde_json::json!({})
    };
    let object = settings
        .as_object_mut()
        .context("Antigravity settings must be a JSON object")?;
    let changed = match provider {
        Some(value) => {
            object.insert("modelProvider".to_owned(), serde_json::json!(value))
                != Some(serde_json::json!(value))
        }
        None => object.remove("modelProvider").is_some(),
    };
    if !changed {
        return Ok(());
    }
    let bytes =
        serde_json::to_vec_pretty(&settings).context("could not serialize Antigravity settings")?;
    crate::live_apply::apply_transaction(user_home, vec![LiveFileChange::write(path, bytes)])
}

pub fn live_state_matches(
    profile_store: &ProfileStore,
    profile_name: &str,
    auth_method: AuthMethod,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<bool> {
    if auth_method == AuthMethod::ApiKey {
        let managed = read_api_key(profile_store, profile_name, backend)?;
        let live = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        let settings = read_settings_file(user_home)?;
        let provider_is_gemini = settings.as_deref().is_some_and(|bytes| {
            serde_json::from_slice::<serde_json::Value>(bytes)
                .ok()
                .and_then(|value| value.get("modelProvider").cloned())
                .and_then(|value| value.as_str().map(|value| value == "gemini"))
                .unwrap_or(false)
        });
        return Ok(!live.is_empty() && managed == live && provider_is_gemini);
    }
    let keyring_ref = read_profile_keyring_ref(profile_store, profile_name)?;
    let managed_secret = read_managed_secret(profile_store, profile_name, backend)?;
    let live_secret =
        super::system_keyring::read_go_secret(&keyring_ref.service, &keyring_ref.account)?;
    let live_secret = live_secret
        .map(|bytes| normalize_secret(&bytes))
        .transpose()?;
    let secrets_match = match (managed_secret.as_deref(), live_secret.as_deref()) {
        (Some(managed), Some(live)) if managed == live => true,
        (Some(managed), Some(live)) => {
            let same_payload = match (
                credential_json_value(managed)?,
                credential_json_value(live)?,
            ) {
                (Some(managed), Some(live)) => managed == live,
                _ => false,
            };
            same_payload || identity::oauth_identity_matches(Tool::Antigravity, managed, live)?
        }
        _ => false,
    };
    let oauth_provider = uses_oauth_provider(read_settings_file(user_home)?.as_deref())?;
    Ok(secrets_match && oauth_provider)
}

pub fn sync_profile_from_live_if_same_identity(
    profile_store: &ProfileStore,
    profile_name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<bool> {
    let Some(snapshot) = live_credentials_snapshot_for_import(user_home)? else {
        return Ok(false);
    };
    let Some(secret) = snapshot.keyring_secret.as_deref() else {
        return Ok(false);
    };
    let Some(managed_secret) = read_managed_secret(profile_store, profile_name, backend)? else {
        return Ok(false);
    };
    if !identity::oauth_identity_matches(Tool::Antigravity, &managed_secret, secret)? {
        return Ok(false);
    }
    persist_profile_keyring_ref(profile_store, profile_name, &snapshot.keyring_ref)?;
    persist_managed_secret(profile_store, profile_name, backend, secret)?;
    Ok(true)
}

pub fn add_oauth_with_backend(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    profile_name: &str,
    label: Option<String>,
    agy_bin: &Path,
    backend: CredentialBackend,
) -> Result<()> {
    let user_home = crate::runtime::user_home().context("could not determine home directory")?;
    let before = capture_live_snapshot(&user_home)?;
    let mut child = Command::new(agy_bin)
        .spawn()
        .with_context(|| format!("could not launch {}", agy_bin.display()))?;
    let status = child.wait_timeout(OAUTH_TIMEOUT)?.unwrap_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        std::process::ExitStatus::from_raw(1 << 8)
    });
    if !status.success() {
        bail!(
            "Antigravity login did not complete successfully.\n  \
             Complete login in the agy session, then retry 'aisw add antigravity {}'.",
            profile_name
        );
    }
    let after = capture_live_snapshot(&user_home)?;
    if before.keyring_secret == after.keyring_secret && before.settings == after.settings {
        bail!(
            "Antigravity login did not produce any new managed state.\n  \
             If agy already signed into the desired account, use 'aisw add antigravity {} --from-live' instead.",
            profile_name
        );
    }

    profile_store.create(Tool::Antigravity, profile_name)?;
    let result = write_profile_snapshot(
        profile_store,
        config_store,
        profile_name,
        label,
        backend,
        &after,
        false,
    );
    if result.is_err() {
        let _ = profile_store.delete(Tool::Antigravity, profile_name);
        if backend == CredentialBackend::SystemKeyring {
            let _ = secure_store::delete_profile_secret(Tool::Antigravity, profile_name);
        }
    }
    result
}

pub fn restore_live_state_after_oauth_add(
    snapshot: Option<LiveSnapshot>,
    user_home: &Path,
) -> Result<()> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    restore_snapshot_to_live(&snapshot, user_home)
}

pub fn restore_snapshot_to_live(snapshot: &LiveSnapshot, user_home: &Path) -> Result<()> {
    let path = live_app_dir(user_home).join("settings.json");
    let change = match &snapshot.settings {
        Some(bytes) => LiveFileChange::write(path, bytes.clone()),
        None => LiveFileChange::delete(path),
    };
    crate::live_apply::apply_transaction(user_home, vec![change])?;
    match snapshot.keyring_secret.as_deref() {
        Some(secret) => super::system_keyring::write_go_secret(
            &snapshot.keyring_ref.service,
            &snapshot.keyring_ref.account,
            &live_secret_bytes(secret)?,
        ),
        None => super::system_keyring::delete_go_secret(
            &snapshot.keyring_ref.service,
            &snapshot.keyring_ref.account,
        ),
    }
}

pub fn emit_shell_env(
    profile_store: &ProfileStore,
    profile_name: &str,
    auth_method: AuthMethod,
    backend: CredentialBackend,
) -> Result<()> {
    if auth_method == AuthMethod::ApiKey {
        let key = read_api_key(profile_store, profile_name, backend)?;
        files::emit_export("GEMINI_API_KEY", &key);
    }
    Ok(())
}

fn persist_profile_keyring_ref(
    profile_store: &ProfileStore,
    profile_name: &str,
    keyring_ref: &KeyringRef,
) -> Result<()> {
    let bytes = serde_json::to_vec(keyring_ref).context("could not serialize keyring metadata")?;
    profile_store.write_file(
        Tool::Antigravity,
        profile_name,
        KEYRING_METADATA_FILE,
        &bytes,
    )
}

pub fn read_profile_keyring_ref(
    profile_store: &ProfileStore,
    profile_name: &str,
) -> Result<KeyringRef> {
    let bytes = profile_store.read_file(Tool::Antigravity, profile_name, KEYRING_METADATA_FILE)?;
    serde_json::from_slice(&bytes).context("could not parse Antigravity keyring metadata")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableKeyringRef {
    service: String,
    account: String,
}

impl serde::Serialize for KeyringRef {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerializableKeyringRef {
            service: self.service.clone(),
            account: self.account.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for KeyringRef {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = SerializableKeyringRef::deserialize(deserializer)?;
        Ok(Self {
            service: value.service,
            account: value.account,
        })
    }
}

trait WaitTimeoutExt {
    fn wait_timeout(&mut self, timeout: Duration) -> Result<Option<std::process::ExitStatus>>;
}

impl WaitTimeoutExt for std::process::Child {
    fn wait_timeout(&mut self, timeout: Duration) -> Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(Some(status));
            }
            if start.elapsed() >= timeout {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }

        fn set_value(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    fn write_live_state(user_home: &Path, secret: &[u8]) {
        fs::create_dir_all(live_app_dir(user_home).join("cache")).unwrap();
        fs::create_dir_all(live_shared_dir(user_home).join("projects")).unwrap();
        fs::write(
            live_app_dir(user_home).join("settings.json"),
            br#"{"theme":"terminal"}"#,
        )
        .unwrap();
        fs::write(
            live_app_dir(user_home).join("cache").join("projects.json"),
            br#"{"current":"repo"}"#,
        )
        .unwrap();
        fs::write(
            live_shared_dir(user_home).join("hooks.json"),
            br#"{"hooks":[]}"#,
        )
        .unwrap();
        fs::write(
            live_shared_dir(user_home)
                .join("projects")
                .join("repo.json"),
            br#"{"mode":"plan"}"#,
        )
        .unwrap();
        super::super::system_keyring::write_go_secret(KEYRING_SERVICE, KEYRING_ACCOUNT, secret)
            .unwrap();
    }

    #[test]
    fn capture_and_apply_round_trip_file_backend() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let home = temp.path().join("home");
        let user_home = temp.path().join("user");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let profile_store = ProfileStore::new(&home);
        let config_store = ConfigStore::new(&home);

        write_live_state(&user_home, br#"{"email":"work@example.com"}"#);
        let snapshot = capture_live_snapshot(&user_home).unwrap();
        profile_store.create(Tool::Antigravity, "work").unwrap();
        write_profile_snapshot(
            &profile_store,
            &config_store,
            "work",
            None,
            CredentialBackend::File,
            &snapshot,
            false,
        )
        .unwrap();

        fs::write(
            live_app_dir(&user_home).join("settings.json"),
            br#"{"theme":"light"}"#,
        )
        .unwrap();
        super::super::system_keyring::write_go_secret(
            KEYRING_SERVICE,
            KEYRING_ACCOUNT,
            br#"{"email":"other@example.com"}"#,
        )
        .unwrap();

        apply_live_credentials(&profile_store, "work", CredentialBackend::File, &user_home)
            .unwrap();
        assert!(live_state_matches(
            &profile_store,
            "work",
            AuthMethod::OAuth,
            CredentialBackend::File,
            &user_home,
        )
        .unwrap());
    }

    #[test]
    fn classify_profile_is_shared_live_oauth() {
        let temp = tempdir().unwrap();
        let profile_store = ProfileStore::new(temp.path());
        let classification = classify_profile(
            &profile_store,
            "work",
            AuthMethod::OAuth,
            CredentialBackend::File,
        )
        .unwrap();
        assert_eq!(
            classification,
            AntigravityAuthClassification::OauthSharedLiveKeyring
        );
        assert_eq!(
            classify_profile(
                &profile_store,
                "work",
                AuthMethod::ApiKey,
                CredentialBackend::File,
            )
            .unwrap(),
            AntigravityAuthClassification::ApiKeyEnvironment
        );
    }

    #[test]
    fn api_key_profile_matches_provider_and_environment() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let user_home = temp.path().join("user");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let profile_store = ProfileStore::new(&home);
        let config_store = ConfigStore::new(&home);
        add_api_key_with_backend(
            &profile_store,
            &config_store,
            "work",
            "AIza-antigravity-test",
            None,
            CredentialBackend::File,
        )
        .unwrap();
        let _key = EnvVarGuard::set_value("GEMINI_API_KEY", "AIza-antigravity-test");
        apply_api_key_settings(&user_home).unwrap();

        assert!(live_state_matches(
            &profile_store,
            "work",
            AuthMethod::ApiKey,
            CredentialBackend::File,
            &user_home,
        )
        .unwrap());
    }

    #[test]
    fn live_credentials_snapshot_returns_none_when_live_state_is_empty() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let user_home = temp.path().join("user");
        fs::create_dir_all(&user_home).unwrap();

        let snapshot = live_credentials_snapshot_for_import(&user_home).unwrap();
        assert!(snapshot.is_none());
    }

    #[test]
    fn write_profile_snapshot_system_keyring_backend_stores_secret_outside_profile_dir() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let home = temp.path().join("home");
        let user_home = temp.path().join("user");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let profile_store = ProfileStore::new(&home);
        let config_store = ConfigStore::new(&home);
        write_live_state(&user_home, br#"{"email":"work@example.com"}"#);
        let snapshot = capture_live_snapshot(&user_home).unwrap();

        profile_store.create(Tool::Antigravity, "work").unwrap();
        write_profile_snapshot(
            &profile_store,
            &config_store,
            "work",
            None,
            CredentialBackend::SystemKeyring,
            &snapshot,
            false,
        )
        .unwrap();

        assert!(!profile_store
            .profile_dir(Tool::Antigravity, "work")
            .join(SECRET_FILE)
            .exists());
        assert_eq!(
            read_managed_secret(&profile_store, "work", CredentialBackend::SystemKeyring)
                .unwrap()
                .unwrap(),
            br#"{"email":"work@example.com"}"#
        );
    }

    #[test]
    fn sync_profile_from_live_if_same_identity_updates_managed_snapshot() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let home = temp.path().join("home");
        let user_home = temp.path().join("user");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let profile_store = ProfileStore::new(&home);
        let config_store = ConfigStore::new(&home);
        write_live_state(
            &user_home,
            br#"{"email":"work@example.com","token":"live"}"#,
        );
        let snapshot = capture_live_snapshot(&user_home).unwrap();

        profile_store.create(Tool::Antigravity, "work").unwrap();
        write_profile_snapshot(
            &profile_store,
            &config_store,
            "work",
            None,
            CredentialBackend::File,
            &snapshot,
            false,
        )
        .unwrap();

        fs::write(
            live_app_dir(&user_home).join("settings.json"),
            br#"{"theme":"light"}"#,
        )
        .unwrap();
        super::super::system_keyring::write_go_secret(
            KEYRING_SERVICE,
            KEYRING_ACCOUNT,
            br#"{"email":"work@example.com","token":"new-live"}"#,
        )
        .unwrap();

        let synced = sync_profile_from_live_if_same_identity(
            &profile_store,
            "work",
            CredentialBackend::File,
            &user_home,
        )
        .unwrap();

        assert!(synced);
        assert_eq!(
            read_managed_secret(&profile_store, "work", CredentialBackend::File)
                .unwrap()
                .unwrap(),
            br#"{"email":"work@example.com","token":"new-live"}"#
        );
        assert!(!profile_store
            .profile_dir(Tool::Antigravity, "work")
            .join("app")
            .exists());
        assert_eq!(
            fs::read(live_app_dir(&user_home).join("settings.json")).unwrap(),
            br#"{"theme":"light"}"#
        );
    }

    #[test]
    fn restore_snapshot_to_live_restores_settings_and_secret_without_deleting_runtime_state() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let user_home = temp.path().join("user");
        fs::create_dir_all(&user_home).unwrap();
        write_live_state(&user_home, br#"{"email":"work@example.com"}"#);

        restore_snapshot_to_live(
            &LiveSnapshot {
                keyring_ref: default_live_keyring_ref(),
                keyring_secret: None,
                settings: None,
            },
            &user_home,
        )
        .unwrap();

        assert!(!live_app_dir(&user_home).join("settings.json").exists());
        assert!(live_app_dir(&user_home)
            .join("cache/projects.json")
            .exists());
        assert!(live_shared_dir(&user_home).join("hooks.json").exists());
        assert!(super::super::system_keyring::read_generic_password(
            KEYRING_SERVICE,
            Some(KEYRING_ACCOUNT),
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn live_state_matches_returns_false_when_live_secret_differs() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let home = temp.path().join("home");
        let user_home = temp.path().join("user");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let profile_store = ProfileStore::new(&home);
        let config_store = ConfigStore::new(&home);
        write_live_state(&user_home, br#"{"email":"work@example.com"}"#);
        let snapshot = capture_live_snapshot(&user_home).unwrap();

        profile_store.create(Tool::Antigravity, "work").unwrap();
        write_profile_snapshot(
            &profile_store,
            &config_store,
            "work",
            None,
            CredentialBackend::File,
            &snapshot,
            false,
        )
        .unwrap();

        super::super::system_keyring::write_go_secret(
            KEYRING_SERVICE,
            KEYRING_ACCOUNT,
            br#"{"email":"other@example.com"}"#,
        )
        .unwrap();

        assert!(!live_state_matches(
            &profile_store,
            "work",
            AuthMethod::OAuth,
            CredentialBackend::File,
            &user_home,
        )
        .unwrap());
    }

    /// Profiles captured before #250 stored the `security` output terminator
    /// along with the secret. Once the reader stopped returning it, a raw byte
    /// comparison would have reported drift on every such profile — so the
    /// managed copy is trimmed on read and repaired on the next persist.
    #[test]
    fn live_state_matches_tolerates_a_legacy_trailing_newline_in_the_managed_secret() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let home = temp.path().join("home");
        let user_home = temp.path().join("user");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let profile_store = ProfileStore::new(&home);
        let config_store = ConfigStore::new(&home);
        write_live_state(&user_home, br#"{"email":"work@example.com"}"#);
        let snapshot = capture_live_snapshot(&user_home).unwrap();

        profile_store.create(Tool::Antigravity, "work").unwrap();
        write_profile_snapshot(
            &profile_store,
            &config_store,
            "work",
            None,
            CredentialBackend::File,
            &snapshot,
            false,
        )
        .unwrap();

        // Re-create the pre-fix on-disk state, bypassing the trimming writer.
        profile_store
            .write_file(
                Tool::Antigravity,
                "work",
                SECRET_FILE,
                b"{\"email\":\"work@example.com\"}\n",
            )
            .unwrap();

        assert!(live_state_matches(
            &profile_store,
            "work",
            AuthMethod::OAuth,
            CredentialBackend::File,
            &user_home,
        )
        .unwrap());
    }

    /// Trimming must not paper over a genuinely different account.
    #[test]
    fn read_managed_secret_trims_without_changing_the_identity() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let profile_store = ProfileStore::new(&home);
        profile_store.create(Tool::Antigravity, "work").unwrap();
        persist_managed_secret(
            &profile_store,
            "work",
            CredentialBackend::File,
            b"\n  {\"email\":\"work@example.com\"}  \n",
        )
        .unwrap();

        let stored = read_managed_secret(&profile_store, "work", CredentialBackend::File)
            .unwrap()
            .unwrap();
        assert_eq!(stored, br#"{"email":"work@example.com"}"#);
        assert_eq!(
            identity::resolve_identity_from_json_bytes(&stored).unwrap(),
            identity::resolve_identity_from_json_bytes(br#"{"email":"work@example.com"}"#).unwrap()
        );
    }
    #[test]
    fn capture_live_snapshot_excludes_nested_runtime_and_shared_files() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", temp.path());
        let user_home = temp.path().join("user");
        write_live_state(&user_home, br#"{"email":"work@example.com"}"#);
        fs::create_dir_all(live_app_dir(&user_home).join("profiles/work")).unwrap();
        fs::write(
            live_app_dir(&user_home).join("profiles/work/settings.json"),
            b"unrelated",
        )
        .unwrap();
        let snapshot = capture_live_snapshot(&user_home).unwrap();
        assert_eq!(snapshot.settings.unwrap(), br#"{"theme":"terminal"}"#);
        assert_eq!(
            fs::read(live_app_dir(&user_home).join("profiles/work/settings.json")).unwrap(),
            b"unrelated"
        );
        assert!(live_shared_dir(&user_home)
            .join("projects/repo.json")
            .exists());
    }

    #[test]
    fn normalize_secret_preserves_envelopes_and_repairs_only_recognizable_hex() {
        let json = br#"{"email":"work@example.com","token":"synthetic"}"#;
        for clean in [
            json.to_vec(),
            format!("go-keyring-base64:{}", STANDARD.encode(json)).into_bytes(),
            format!(
                "go-keyring-encoded:{}",
                json.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )
            .into_bytes(),
        ] {
            let mut legacy = clean.clone();
            legacy.push(b'\n');
            for _ in 0..3 {
                legacy = legacy
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
                    .into_bytes();
                let normalized = normalize_secret(&legacy).unwrap();
                assert_eq!(normalized, clean);
                assert_eq!(normalize_secret(&normalized).unwrap(), clean);
                assert_eq!(
                    credential_json_value(&normalized).unwrap(),
                    Some(serde_json::from_slice(json).unwrap())
                );
            }
        }
        assert_eq!(normalize_secret(b"deadbeef").unwrap(), b"deadbeef");
        assert!(normalize_secret(b"go-keyring-base64:not-base64").is_err());
        assert!(normalize_secret(b"go-keyring-encoded:7b").is_err());
    }
}
