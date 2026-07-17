use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
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
const APP_PREFIX: &str = "app";
const SHARED_PREFIX: &str = "shared";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(180);
const KEYRING_SERVICE: &str = "gemini";
const KEYRING_ACCOUNT: &str = "antigravity";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityAuthClassification {
    OauthSharedLiveKeyring,
}

impl AntigravityAuthClassification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OauthSharedLiveKeyring => "oauth_shared_live_keyring",
        }
    }

    pub fn human_label(self) -> &'static str {
        match self {
            Self::OauthSharedLiveKeyring => "OAuth shared live keyring",
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
    pub app_files: BTreeMap<String, Vec<u8>>,
    pub shared_files: BTreeMap<String, Vec<u8>>,
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
    if auth_method != AuthMethod::OAuth {
        bail!("Antigravity currently supports OAuth profiles only");
    }
    Ok(AntigravityAuthClassification::OauthSharedLiveKeyring)
}

pub fn read_managed_secret(
    profile_store: &ProfileStore,
    profile_name: &str,
    backend: CredentialBackend,
) -> Result<Option<Vec<u8>>> {
    match backend {
        CredentialBackend::File => {
            let path = profile_store
                .profile_dir(Tool::Antigravity, profile_name)
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
    }
}

pub fn persist_managed_secret(
    profile_store: &ProfileStore,
    profile_name: &str,
    backend: CredentialBackend,
    secret: &[u8],
) -> Result<()> {
    match backend {
        CredentialBackend::File => {
            profile_store.write_file(Tool::Antigravity, profile_name, SECRET_FILE, secret)
        }
        CredentialBackend::SystemKeyring => {
            secure_store::write_profile_secret(Tool::Antigravity, profile_name, secret)
        }
    }
}

pub fn live_credentials_snapshot_for_import(user_home: &Path) -> Result<Option<LiveSnapshot>> {
    let snapshot = capture_live_snapshot(user_home)?;
    if snapshot.keyring_secret.is_none()
        && snapshot.app_files.is_empty()
        && snapshot.shared_files.is_empty()
    {
        return Ok(None);
    }
    Ok(Some(snapshot))
}

pub fn capture_live_snapshot(user_home: &Path) -> Result<LiveSnapshot> {
    let keyring_ref = default_live_keyring_ref();
    Ok(LiveSnapshot {
        keyring_secret: super::system_keyring::read_generic_password(
            &keyring_ref.service,
            Some(&keyring_ref.account),
        )?,
        keyring_ref,
        app_files: read_live_dir(&live_app_dir(user_home))?,
        shared_files: read_live_dir(&live_shared_dir(user_home))?,
    })
}

fn read_live_dir(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    if !dir.exists() {
        return Ok(BTreeMap::new());
    }
    let mut files_map = BTreeMap::new();
    for file in files::list_regular_files_recursive(dir)? {
        let relative = file.file_name.to_string_lossy().into_owned();
        let bytes = fs::read(&file.path)
            .with_context(|| format!("could not read {}", file.path.display()))?;
        files_map.insert(relative, bytes);
    }
    Ok(files_map)
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

    clear_profile_subtree(profile_store, profile_name, APP_PREFIX)?;
    clear_profile_subtree(profile_store, profile_name, SHARED_PREFIX)?;
    persist_profile_tree(profile_store, profile_name, APP_PREFIX, &snapshot.app_files)?;
    persist_profile_tree(
        profile_store,
        profile_name,
        SHARED_PREFIX,
        &snapshot.shared_files,
    )?;

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

fn clear_profile_subtree(
    profile_store: &ProfileStore,
    profile_name: &str,
    prefix: &str,
) -> Result<()> {
    let dir = profile_store
        .profile_dir(Tool::Antigravity, profile_name)
        .join(prefix);
    if dir.exists() {
        fs::remove_dir_all(&dir).with_context(|| format!("could not delete {}", dir.display()))?;
    }
    Ok(())
}

fn persist_profile_tree(
    profile_store: &ProfileStore,
    profile_name: &str,
    prefix: &str,
    files_map: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for (relative, bytes) in files_map {
        let stored = format!("{prefix}/{relative}");
        profile_store.write_file(Tool::Antigravity, profile_name, &stored, bytes)?;
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

    let changes = build_apply_transaction(profile_store, profile_name, user_home)?;
    crate::live_apply::apply_transaction(changes)?;
    super::system_keyring::upsert_generic_password(
        &keyring_ref.service,
        &keyring_ref.account,
        &secret,
    )
}

fn build_apply_transaction(
    profile_store: &ProfileStore,
    profile_name: &str,
    user_home: &Path,
) -> Result<Vec<LiveFileChange>> {
    let stored_app = profile_tree_map(profile_store, profile_name, APP_PREFIX)?;
    let stored_shared = profile_tree_map(profile_store, profile_name, SHARED_PREFIX)?;
    let mut changes = Vec::new();
    changes.extend(sync_dir_to_live(
        &stored_app,
        &live_app_dir(user_home),
        &read_live_dir(&live_app_dir(user_home))?,
    ));
    changes.extend(sync_dir_to_live(
        &stored_shared,
        &live_shared_dir(user_home),
        &read_live_dir(&live_shared_dir(user_home))?,
    ));
    Ok(changes)
}

fn sync_dir_to_live(
    stored: &BTreeMap<String, Vec<u8>>,
    live_root: &Path,
    live: &BTreeMap<String, Vec<u8>>,
) -> Vec<LiveFileChange> {
    let mut changes = Vec::new();
    for (relative, bytes) in stored {
        let live_bytes = live.get(relative);
        if live_bytes != Some(bytes) {
            changes.push(LiveFileChange::write(
                live_root.join(relative),
                bytes.clone(),
            ));
        }
    }
    for relative in live.keys() {
        if !stored.contains_key(relative) {
            changes.push(LiveFileChange::delete(live_root.join(relative)));
        }
    }
    changes
}

fn profile_tree_map(
    profile_store: &ProfileStore,
    profile_name: &str,
    prefix: &str,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let root = profile_store
        .profile_dir(Tool::Antigravity, profile_name)
        .join(prefix);
    if !root.exists() {
        return Ok(BTreeMap::new());
    }
    let mut files_map = BTreeMap::new();
    for file in files::list_regular_files_recursive(&root)? {
        let relative = file.file_name.to_string_lossy().into_owned();
        let stored = format!("{prefix}/{relative}");
        let bytes = profile_store.read_file(Tool::Antigravity, profile_name, &stored)?;
        files_map.insert(relative, bytes);
    }
    Ok(files_map)
}

pub fn live_state_matches(
    profile_store: &ProfileStore,
    profile_name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<bool> {
    let keyring_ref = read_profile_keyring_ref(profile_store, profile_name)?;
    let managed_secret = read_managed_secret(profile_store, profile_name, backend)?;
    let live_secret = super::system_keyring::read_generic_password(
        &keyring_ref.service,
        Some(&keyring_ref.account),
    )?;
    if managed_secret != live_secret {
        return Ok(false);
    }
    Ok(profile_tree_map(profile_store, profile_name, APP_PREFIX)?
        == read_live_dir(&live_app_dir(user_home))?
        && profile_tree_map(profile_store, profile_name, SHARED_PREFIX)?
            == read_live_dir(&live_shared_dir(user_home))?)
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
    let managed_identity = identity::resolve_identity_from_json_bytes(&managed_secret)?;
    let live_identity = identity::resolve_identity_from_json_bytes(secret)?;
    if managed_identity.is_none() || managed_identity != live_identity {
        return Ok(false);
    }
    persist_profile_keyring_ref(profile_store, profile_name, &snapshot.keyring_ref)?;
    persist_managed_secret(profile_store, profile_name, backend, secret)?;
    clear_profile_subtree(profile_store, profile_name, APP_PREFIX)?;
    clear_profile_subtree(profile_store, profile_name, SHARED_PREFIX)?;
    persist_profile_tree(profile_store, profile_name, APP_PREFIX, &snapshot.app_files)?;
    persist_profile_tree(
        profile_store,
        profile_name,
        SHARED_PREFIX,
        &snapshot.shared_files,
    )?;
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
    let user_home = dirs::home_dir().context("could not determine home directory")?;
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
    if before.keyring_secret == after.keyring_secret
        && before.app_files == after.app_files
        && before.shared_files == after.shared_files
    {
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
    let changes = {
        let mut changes = Vec::new();
        changes.extend(sync_dir_to_live(
            &snapshot.app_files,
            &live_app_dir(user_home),
            &read_live_dir(&live_app_dir(user_home))?,
        ));
        changes.extend(sync_dir_to_live(
            &snapshot.shared_files,
            &live_shared_dir(user_home),
            &read_live_dir(&live_shared_dir(user_home))?,
        ));
        changes
    };
    crate::live_apply::apply_transaction(changes)?;
    match snapshot.keyring_secret.as_deref() {
        Some(secret) => super::system_keyring::upsert_generic_password(
            &snapshot.keyring_ref.service,
            &snapshot.keyring_ref.account,
            secret,
        ),
        None => super::system_keyring::delete_generic_password(
            &snapshot.keyring_ref.service,
            &snapshot.keyring_ref.account,
        ),
    }
}

pub fn emit_shell_env() {}

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
        super::super::system_keyring::upsert_generic_password(
            KEYRING_SERVICE,
            KEYRING_ACCOUNT,
            secret,
        )
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
        super::super::system_keyring::upsert_generic_password(
            KEYRING_SERVICE,
            KEYRING_ACCOUNT,
            br#"{"email":"other@example.com"}"#,
        )
        .unwrap();

        apply_live_credentials(&profile_store, "work", CredentialBackend::File, &user_home)
            .unwrap();
        assert!(
            live_state_matches(&profile_store, "work", CredentialBackend::File, &user_home)
                .unwrap()
        );
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
        super::super::system_keyring::upsert_generic_password(
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
        assert_eq!(
            profile_store
                .read_file(Tool::Antigravity, "work", "app/settings.json")
                .unwrap(),
            br#"{"theme":"light"}"#
        );
    }

    #[test]
    fn restore_snapshot_to_live_removes_stale_files_and_deletes_secret() {
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
                app_files: BTreeMap::new(),
                shared_files: BTreeMap::new(),
            },
            &user_home,
        )
        .unwrap();

        assert!(read_live_dir(&live_app_dir(&user_home)).unwrap().is_empty());
        assert!(read_live_dir(&live_shared_dir(&user_home))
            .unwrap()
            .is_empty());
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

        super::super::system_keyring::upsert_generic_password(
            KEYRING_SERVICE,
            KEYRING_ACCOUNT,
            br#"{"email":"other@example.com"}"#,
        )
        .unwrap();

        assert!(
            !live_state_matches(&profile_store, "work", CredentialBackend::File, &user_home)
                .unwrap()
        );
    }
}
