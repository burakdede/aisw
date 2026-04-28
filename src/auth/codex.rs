use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;

use super::files;
use super::identity;
use super::secure_store;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::live_apply::LiveFileChange;
use crate::profile::ProfileStore;
use crate::types::{StateMode, Tool};

pub(crate) const AUTH_FILE: &str = "auth.json";
const CONFIG_FILE: &str = "config.toml";

// Codex reads credentials from a file rather than the OS keyring when this is set.
// aisw always enforces file-backed auth: Codex's keyring account key is a SHA-256
// hash of the canonical CODEX_HOME path, which aisw cannot reconstruct portably.
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

pub struct LiveCredentialSnapshot {
    pub bytes: Vec<u8>,
    pub source_path: PathBuf,
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
    let Some(_) = live_local_state_dir(user_home) else {
        return Ok(None);
    };

    // Codex defaults to file-backed auth (CODEX_HOME/auth.json). Even when
    // keyring mode is configured, aisw cannot safely read or write those entries
    // because Codex's keyring account key is a SHA-256 hash of the canonical
    // CODEX_HOME path — not a username or email that aisw can reconstruct.
    let auth_path = live_auth_path(user_home);
    if !auth_path.exists() {
        return Ok(None);
    }

    let bytes =
        fs::read(&auth_path).with_context(|| format!("could not read {}", auth_path.display()))?;
    Ok(Some(LiveCredentialSnapshot {
        bytes,
        source_path: auth_path,
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

    files::cleanup_profile_on_error(
        write_file_store_config(profile_store, name),
        profile_store,
        Tool::Codex,
        name,
    )?;

    let auth_json = serde_json::to_string(&serde_json::json!({ "token": key }))
        .context("could not serialize API key credentials")?;
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
    store_oauth_profile(profile_store, config_store, name, label, &auth_bytes).inspect_err(
        |_| {
            let _ = fs::remove_dir_all(&capture_dir);
        },
    )?;
    let _ = fs::remove_dir_all(&capture_dir);

    Ok(())
}

fn store_oauth_profile(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    auth_bytes: &[u8],
) -> Result<()> {
    files::cleanup_profile_on_error(
        persist_oauth_storage(profile_store, name, auth_bytes),
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
            CredentialBackend::File,
        ),
        profile_store,
        Tool::Codex,
        name,
    )?;

    config_store
        .add_profile(
            Tool::Codex,
            name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: CredentialBackend::File,
                label,
            },
        )
        .inspect_err(|_| {
            let _ = profile_store.delete(Tool::Codex, name);
        })?;

    Ok(())
}

fn persist_oauth_storage(
    profile_store: &ProfileStore,
    name: &str,
    auth_bytes: &[u8],
) -> Result<()> {
    write_file_store_config(profile_store, name)?;
    profile_store.write_file(Tool::Codex, name, AUTH_FILE, auth_bytes)
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
    let _spinner = crate::output::start_spinner("Waiting for Codex login to complete...");

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

        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("could not poll {}", codex_bin.display()))?
        {
            let exit_note = if status.success() {
                "Codex exited"
            } else {
                "Codex exited with an error"
            };
            bail!(
                "{} before aisw could capture credentials.\n  \
                 Codex login completed without writing auth.json into CODEX_HOME.\n  \
                 If your Codex build stores auth somewhere else, use an API key instead.",
                exit_note
            );
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

fn read_stored_credentials(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
) -> Result<Vec<u8>> {
    match backend {
        CredentialBackend::File => profile_store.read_file(Tool::Codex, name, AUTH_FILE),
        CredentialBackend::SystemKeyring => {
            secure_store::read_profile_secret(Tool::Codex, name)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "secure credentials for codex profile '{}' are missing from the managed system keyring",
                    name
                )
            })
        }
    }
}

pub fn apply_live_credentials(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<()> {
    let live_dir = live_dir(user_home);
    std::fs::create_dir_all(&live_dir)
        .with_context(|| format!("could not create {}", live_dir.display()))?;

    let auth_bytes = read_stored_credentials(profile_store, name, backend)?;
    let auth_dest = live_auth_path(user_home);
    let config_dest = live_config_path(user_home);
    let config_bytes = desired_live_file_store_config(user_home)?.into_bytes();

    crate::live_apply::apply_transaction(vec![
        LiveFileChange::write(auth_dest, auth_bytes),
        LiveFileChange::write(config_dest, config_bytes),
    ])
}

pub fn apply_live_files(profile_store: &ProfileStore, name: &str, user_home: &Path) -> Result<()> {
    apply_live_credentials(profile_store, name, CredentialBackend::File, user_home)
}

pub fn emit_shell_env(name: &str, profile_store: &ProfileStore, mode: StateMode) {
    match mode {
        StateMode::Isolated => {
            let profile_dir = profile_store.profile_dir(Tool::Codex, name);
            files::emit_export("CODEX_HOME", &profile_dir.display().to_string());
        }
        StateMode::Shared => {
            files::emit_unset("CODEX_HOME");
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
    let live_auth = live_auth_path(user_home);
    if !live_auth.exists() {
        return Ok(false);
    }
    let stored = read_stored_credentials(profile_store, name, backend)?;
    let live = std::fs::read(&live_auth)
        .with_context(|| format!("could not read {}", live_auth.display()))?;

    let stored_value: serde_json::Value = serde_json::from_slice(&stored).ok().unwrap_or_default();
    let live_value: serde_json::Value = serde_json::from_slice(&live).ok().unwrap_or_default();

    if stored_value != live_value {
        return Ok(false);
    }
    Ok(config_uses_file_store(&config))
}

// ---- Automatic synchronization ----

pub fn sync_profile_from_live_if_same_identity(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<bool> {
    let Some(snapshot) = live_credentials_snapshot_for_import(user_home)? else {
        return Ok(false);
    };

    let stored_bytes = read_stored_credentials(profile_store, name, backend)?;
    let Some(stored_identity) = identity::resolve_identity_from_json_bytes(&stored_bytes)? else {
        return Ok(false);
    };
    let Some(live_identity) = identity::resolve_identity_from_json_bytes(&snapshot.bytes)? else {
        return Ok(false);
    };

    if stored_identity != live_identity {
        return Ok(false);
    }

    persist_oauth_storage(profile_store, name, &snapshot.bytes)?;
    Ok(true)
}

#[cfg(test)]
mod sync_tests {
    use super::*;
    use crate::config::ConfigStore;
    use tempfile::tempdir;

    fn stores(dir: &std::path::Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
    }

    #[test]
    fn sync_profile_from_live_updates_when_identity_matches() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(user_home.join(".codex")).unwrap();

        let (ps, cs) = stores(&home);
        let name = "work";

        // 1. Create a profile with old credentials
        ps.create(Tool::Codex, name).unwrap();
        let old_auth = br#"{"token":"old-token","account":{"email":"user@example.com"}}"#;
        ps.write_file(Tool::Codex, name, AUTH_FILE, old_auth)
            .unwrap();
        cs.add_profile(
            Tool::Codex,
            name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: CredentialBackend::File,
                label: None,
            },
        )
        .unwrap();

        // 2. Prepare live state with new token but SAME identity
        let new_auth = br#"{"token":"new-token","account":{"email":"user@example.com"}}"#;
        fs::write(user_home.join(".codex").join(AUTH_FILE), new_auth).unwrap();

        // 3. Trigger sync
        let result =
            sync_profile_from_live_if_same_identity(&ps, name, CredentialBackend::File, &user_home)
                .unwrap();

        assert!(result, "sync should have returned true");
        let stored = ps.read_file(Tool::Codex, name, AUTH_FILE).unwrap();
        assert_eq!(stored, new_auth);
    }

    #[test]
    fn sync_profile_from_live_skips_when_identity_differs() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(user_home.join(".codex")).unwrap();

        let (ps, cs) = stores(&home);
        let name = "work";

        ps.create(Tool::Codex, name).unwrap();
        let old_auth = br#"{"token":"old-token","account":{"email":"user@example.com"}}"#;
        ps.write_file(Tool::Codex, name, AUTH_FILE, old_auth)
            .unwrap();
        cs.add_profile(
            Tool::Codex,
            name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: CredentialBackend::File,
                label: None,
            },
        )
        .unwrap();

        // New token for DIFFERENT identity
        let diff_auth = br#"{"token":"new-token","account":{"email":"other@example.com"}}"#;
        fs::write(user_home.join(".codex").join(AUTH_FILE), diff_auth).unwrap();

        let result =
            sync_profile_from_live_if_same_identity(&ps, name, CredentialBackend::File, &user_home)
                .unwrap();

        assert!(!result, "sync should have skipped (returned false)");
        let stored = ps.read_file(Tool::Codex, name, AUTH_FILE).unwrap();
        assert_eq!(stored, old_auth);
    }
}

fn desired_live_file_store_config(user_home: &Path) -> Result<String> {
    let config_dest = live_config_path(user_home);
    if config_dest.exists() {
        let current = std::fs::read_to_string(&config_dest)
            .with_context(|| format!("could not read {}", config_dest.display()))?;
        // Validate before merging: a corrupt config.toml must not be silently
        // overwritten with a line-merged result that may still be invalid TOML.
        toml::from_str::<toml::Value>(&current).map_err(|e| {
            anyhow::anyhow!(
                "Codex config.toml is not valid TOML — cannot merge required settings.\n  \
                 Fix or remove {} and retry.\n  ({})",
                config_dest.display(),
                e
            )
        })?;
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
            lines.push("cli_auth_credentials_store = \"file\"".to_owned());
            replaced = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !replaced {
        if !current.is_empty() && !current.ends_with('\n') {
            lines.push(String::new());
        }
        lines.push("cli_auth_credentials_store = \"file\"".to_owned());
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
        assert_eq!(
            snapshot.source_path,
            user_home.join(".codex").join(AUTH_FILE)
        );
    }

    #[test]
    fn live_credentials_snapshot_returns_none_when_no_auth_json() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        // ~/.codex exists but no auth.json — should return None.
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();

        assert!(live_credentials_snapshot_for_import(&user_home)
            .unwrap()
            .is_none());
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
            "[ \"$1\" = \"login\" ] || exit 9\n\
             mkdir -p \"$CODEX_HOME\"\n\
             echo '{\"token\":\"tok\"}' > \"$CODEX_HOME/auth.json\"\n\
             exit 0\n"
        } else {
            "[ \"$1\" = \"login\" ] || exit 9\n\
             exit 0\n"
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
             [ \"$1\" = \"login\" ] || exit 9\n\
             mkdir -p \"$CODEX_HOME\"\n\
             [ -f \"$CODEX_HOME/config.toml\" ] && touch \"$CODEX_HOME/../config_was_present\"\n\
             echo '{}' > \"$CODEX_HOME/auth.json\"\n\
             exit 0\n",
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

        let message = err.to_string();
        assert!(
            message.contains("exited before aisw could capture credentials")
                || message.contains("timed out"),
            "unexpected error: {message}"
        );
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
    fn oauth_uses_standard_login_command() {
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
             mkdir -p \"$CODEX_HOME\"\n\
             printf '%s %s' \"$1\" \"$2\" > \"$CODEX_HOME/../login_args\"\n\
             echo '{}' > \"$CODEX_HOME/auth.json\"\n\
             exit 0\n",
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

        let sentinel = ps.profile_dir(Tool::Codex, "main").join("login_args");
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "login ");
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

    // ---- merge_file_store_config tests ----

    #[test]
    fn merge_file_store_config_replaces_existing_key() {
        let input = "model = \"gpt-5.4\"\ncli_auth_credentials_store = \"keyring\"\n";
        let output = merge_file_store_config(input);
        assert_eq!(
            output,
            "model = \"gpt-5.4\"\ncli_auth_credentials_store = \"file\"\n"
        );
    }

    #[test]
    fn merge_file_store_config_appends_when_key_absent() {
        let input = "model = \"gpt-5.4\"\n";
        let output = merge_file_store_config(input);
        assert_eq!(
            output,
            "model = \"gpt-5.4\"\ncli_auth_credentials_store = \"file\"\n"
        );
    }

    #[test]
    fn merge_file_store_config_handles_no_trailing_newline() {
        // The function inserts a blank line when the existing content has no trailing
        // newline, to ensure the appended key is separated from the final line.
        let input = "model = \"gpt-5.4\"";
        let output = merge_file_store_config(input);
        assert_eq!(
            output,
            "model = \"gpt-5.4\"\n\ncli_auth_credentials_store = \"file\"\n"
        );
    }

    #[test]
    fn merge_file_store_config_is_idempotent() {
        let first = merge_file_store_config("model = \"gpt-5.4\"\n");
        let second = merge_file_store_config(&first);
        assert_eq!(first, second);
    }

    // ---- config_uses_file_store tests ----

    #[test]
    fn config_uses_file_store_returns_true_for_file() {
        assert!(config_uses_file_store(
            "cli_auth_credentials_store = \"file\"\n"
        ));
    }

    #[test]
    fn config_uses_file_store_returns_false_for_keyring() {
        assert!(!config_uses_file_store(
            "cli_auth_credentials_store = \"keyring\"\n"
        ));
    }

    #[test]
    fn config_uses_file_store_returns_false_when_absent() {
        assert!(!config_uses_file_store("model = \"gpt-5.4\"\n"));
    }

    // ---- live_files_match tests ----

    #[test]
    fn live_files_match_returns_true_when_auth_and_config_match() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();
        apply_live_files(&ps, "main", &user_home).unwrap();

        assert!(live_files_match(&ps, "main", CredentialBackend::File, &user_home).unwrap());
    }

    #[test]
    fn live_files_match_returns_false_when_config_absent() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();
        // write auth.json but no config.toml
        let auth_bytes = ps.read_file(Tool::Codex, "main", AUTH_FILE).unwrap();
        std::fs::write(user_home.join(".codex").join(AUTH_FILE), &auth_bytes).unwrap();

        assert!(!live_files_match(&ps, "main", CredentialBackend::File, &user_home).unwrap());
    }

    #[test]
    fn live_files_match_returns_false_when_auth_differs() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();
        apply_live_files(&ps, "main", &user_home).unwrap();
        // Overwrite live auth.json with a different key.
        std::fs::write(
            user_home.join(".codex").join(AUTH_FILE),
            br#"{"token":"sk-different-key"}"#,
        )
        .unwrap();

        assert!(!live_files_match(&ps, "main", CredentialBackend::File, &user_home).unwrap());
    }

    // ---- desired_live_file_store_config / TOML validation tests ----

    #[test]
    fn apply_live_files_rejects_malformed_config_toml() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();
        // Write a config.toml that is not valid TOML.
        std::fs::write(
            user_home.join(".codex").join(CONFIG_FILE),
            b"this is not = valid [ toml !!!\n",
        )
        .unwrap();

        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();

        let err = apply_live_files(&ps, "main", &user_home).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not valid TOML"),
            "expected TOML validation error, got: {msg}"
        );
        // The original file must be untouched — live_apply transaction never committed.
        let on_disk = std::fs::read_to_string(user_home.join(".codex").join(CONFIG_FILE)).unwrap();
        assert!(
            on_disk.contains("this is not"),
            "original config.toml was modified despite error"
        );
    }

    #[test]
    fn apply_live_files_merges_into_existing_valid_config() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();
        // Simulate a user's existing Codex config with custom settings.
        std::fs::write(
            user_home.join(".codex").join(CONFIG_FILE),
            b"model = \"o3\"\n[features]\nunified_exec = true\n",
        )
        .unwrap();

        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();
        apply_live_files(&ps, "main", &user_home).unwrap();

        let contents = std::fs::read_to_string(user_home.join(".codex").join(CONFIG_FILE)).unwrap();
        assert!(contents.contains("model = \"o3\""), "model key lost");
        assert!(contents.contains("[features]"), "[features] section lost");
        assert!(
            contents.contains("unified_exec = true"),
            "unified_exec key lost"
        );
        assert!(
            contents.contains("cli_auth_credentials_store = \"file\""),
            "required key not present"
        );
    }

    #[test]
    fn apply_live_files_replaces_keyring_store_with_file() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();
        std::fs::write(
            user_home.join(".codex").join(CONFIG_FILE),
            b"cli_auth_credentials_store = \"keyring\"\n",
        )
        .unwrap();

        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();
        apply_live_files(&ps, "main", &user_home).unwrap();

        let contents = std::fs::read_to_string(user_home.join(".codex").join(CONFIG_FILE)).unwrap();
        assert!(
            contents.contains("cli_auth_credentials_store = \"file\""),
            "keyring not replaced with file"
        );
        assert!(
            !contents.contains("keyring"),
            "keyring value still present after merge"
        );
    }

    #[test]
    fn apply_live_files_creates_config_when_absent() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        // No pre-existing config.toml.
        std::fs::create_dir_all(user_home.join(".codex")).unwrap();

        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "main", valid_key(), None).unwrap();
        apply_live_files(&ps, "main", &user_home).unwrap();

        let contents = std::fs::read_to_string(user_home.join(".codex").join(CONFIG_FILE)).unwrap();
        assert_eq!(contents, CONFIG_TOML_CONTENTS);
    }
}
