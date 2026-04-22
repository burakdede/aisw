//! OAuth capture flow and account-metadata persistence for Claude Code.
//!
//! Claude's OAuth flow varies by installation and platform:
//!  - We monitor Claude's live credential locations (file and keychain) for
//!    changes after `claude auth login` completes.
//!  - We avoid setting `CLAUDE_CONFIG_DIR` during login so Claude can run its
//!    native auth flow without override-induced fallbacks.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::output;
use crate::profile::ProfileStore;
use crate::types::Tool;

use super::super::files;
use super::super::identity;
use super::super::secure_store;
use super::keychain::{
    forced_auth_storage, oauth_stored_backend, read_keychain_credentials, storage_fallback_note,
    watch_keychain_during_oauth, ClaudeAuthStorage,
};
use super::paths::{
    live_account_metadata_path, live_credentials_path, live_credentials_paths, live_local_state_dir,
};
use super::{read_stored_credentials, LiveCredentialSnapshot, LiveCredentialSource};

fn persist_oauth_storage(
    profile_store: &ProfileStore,
    name: &str,
    stored_backend: CredentialBackend,
    auth_bytes: &[u8],
) -> Result<()> {
    match stored_backend {
        CredentialBackend::File => {
            profile_store.write_file(Tool::Claude, name, super::CREDENTIALS_FILE, auth_bytes)
        }
        CredentialBackend::SystemKeyring => {
            secure_store::write_profile_secret(Tool::Claude, name, auth_bytes)
        }
    }
}

// ---- Live credential snapshot (for import) ----

/// Reads the current live credentials from disk or keychain, returning a
/// snapshot suitable for profile import. Returns `None` when no credentials
/// are present.
pub fn live_credentials_snapshot_for_import(
    user_home: &Path,
) -> Result<Option<LiveCredentialSnapshot>> {
    use super::keychain::read_live_keychain_credentials_for_import;

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

// ---- OAuth account metadata ----

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

/// Reads the live OAuth account metadata for profile import.
pub fn read_live_oauth_account_metadata_for_import(user_home: &Path) -> Result<Option<Vec<u8>>> {
    read_live_oauth_account_metadata(user_home)
}

fn restore_live_credentials_snapshot(
    snapshot: Option<LiveCredentialSnapshot>,
    user_home: &Path,
) -> Result<()> {
    match snapshot {
        Some(snapshot) => match snapshot.source {
            LiveCredentialSource::File(path) => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("could not create {}", parent.display()))?;
                }
                fs::write(&path, snapshot.bytes)
                    .with_context(|| format!("could not write {}", path.display()))?;
                files::set_permissions_600(&path)?;
            }
            LiveCredentialSource::Keychain => {
                super::keychain::write_keychain_credentials(&snapshot.bytes)?;
            }
        },
        None => {
            for path in live_credentials_paths(user_home) {
                if path.exists() {
                    fs::remove_file(&path)
                        .with_context(|| format!("could not remove {}", path.display()))?;
                }
            }
            // Best effort cleanup for environments where Claude stores auth in keychain.
            let _ = super::keychain::delete_keychain_credentials();
        }
    }
    Ok(())
}

fn restore_live_oauth_account_metadata_snapshot(
    metadata: Option<Vec<u8>>,
    user_home: &Path,
) -> Result<()> {
    let live_path = live_account_metadata_path(user_home);
    if metadata.is_none() && !live_path.exists() {
        return Ok(());
    }
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
        if let Some(metadata) = metadata {
            let oauth_account_value: serde_json::Value = serde_json::from_slice(&metadata)
                .context("could not parse Claude oauthAccount metadata snapshot")?;
            obj.insert("oauthAccount".to_owned(), oauth_account_value);
        } else {
            obj.remove("oauthAccount");
        }
    }

    let bytes = serde_json::to_vec_pretty(&live_json)
        .context("could not serialize Claude metadata for live-state restore")?;
    fs::write(&live_path, bytes)
        .with_context(|| format!("could not write {}", live_path.display()))?;
    files::set_permissions_600(&live_path)
}

/// Restores Claude live credentials/metadata after an OAuth add that should not
/// switch the currently active live account.
pub fn restore_live_state_after_oauth_add(
    snapshot: Option<LiveCredentialSnapshot>,
    oauth_account_metadata: Option<Vec<u8>>,
    user_home: &Path,
) -> Result<()> {
    restore_live_credentials_snapshot(snapshot, user_home)?;
    restore_live_oauth_account_metadata_snapshot(oauth_account_metadata, user_home)
}

fn persist_live_oauth_account_metadata(
    profile_store: &ProfileStore,
    name: &str,
    user_home: &Path,
) -> Result<()> {
    let Some(metadata) = read_live_oauth_account_metadata(user_home)? else {
        return Ok(());
    };
    profile_store.write_file(Tool::Claude, name, super::OAUTH_ACCOUNT_FILE, &metadata)
}

/// Captures the current live OAuth account metadata into the named profile.
pub fn capture_live_oauth_account_metadata(
    profile_store: &ProfileStore,
    name: &str,
    user_home: &Path,
) -> Result<()> {
    persist_live_oauth_account_metadata(profile_store, name, user_home)
}

pub(super) fn apply_live_oauth_account_metadata(
    profile_store: &ProfileStore,
    name: &str,
    user_home: &Path,
) -> Result<()> {
    let profile_path = profile_store
        .profile_dir(Tool::Claude, name)
        .join(super::OAUTH_ACCOUNT_FILE);
    if !profile_path.exists() {
        return Ok(());
    }

    let oauth_account = profile_store.read_file(Tool::Claude, name, super::OAUTH_ACCOUNT_FILE)?;
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

// ---- OAuth add flow ----

/// Start the Claude OAuth flow using the installed `claude` binary.
///
/// Spawns `claude auth login` and polls for credential changes in the live
/// file path and the OS keychain (where available).
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
        super::OAUTH_TIMEOUT,
        super::POLL_INTERVAL,
    )
}

pub(super) fn add_oauth_with(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    claude_bin: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<()> {
    profile_store.create(Tool::Claude, name)?;
    let stored_backend = oauth_stored_backend();

    if let Some(note) = storage_fallback_note(CredentialBackend::SystemKeyring) {
        output::print_warning(note);
    }

    let auth_bytes = files::cleanup_profile_on_error(
        run_oauth_flow(claude_bin, timeout, poll_interval),
        profile_store,
        Tool::Claude,
        name,
    )?;

    files::cleanup_profile_on_error(
        persist_oauth_storage(profile_store, name, stored_backend, &auth_bytes),
        profile_store,
        Tool::Claude,
        name,
    )?;

    if let Some(user_home) = dirs::home_dir() {
        files::cleanup_profile_on_error(
            persist_live_oauth_account_metadata(profile_store, name, &user_home),
            profile_store,
            Tool::Claude,
            name,
        )?;
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

    Ok(())
}

fn run_oauth_flow(
    claude_bin: &Path,
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
    let _spinner = crate::output::start_spinner("Waiting for Claude login...");
    output::print_warning(
        "Claude may reuse the account already signed in in your browser. \
If you need a different Claude account, fully sign out of claude.com first, then rerun \
'aisw add claude <name>'.",
    );

    // CLAUDE_CONFIG_DIR is intentionally NOT set here so Claude can run its
    // native auth flow. We detect completion by watching the live credential
    // file and the OS keychain for changes.
    let live_credentials_path_before = dirs::home_dir().map(|home| live_credentials_path(&home));
    let file_before = live_credentials_path_before
        .as_ref()
        .filter(|path| path.exists())
        .map(fs::read)
        .transpose()
        .with_context(|| {
            live_credentials_path_before
                .as_ref()
                .map(|path| format!("could not read {}", path.display()))
                .unwrap_or_else(|| "could not read Claude live credentials".to_owned())
        })?;

    let mut cmd = Command::new(claude_bin);
    cmd.arg("auth").arg("login");
    let mut child = cmd
        .spawn()
        .with_context(|| format!("could not spawn {}", claude_bin.display()))?;

    let deadline = Instant::now() + timeout;

    loop {
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

        if let Some(live_path) = &live_credentials_path_before {
            if live_path.exists() {
                let current = fs::read(live_path)
                    .with_context(|| format!("could not read {}", live_path.display()))?;
                let changed = file_before.as_deref() != Some(current.as_slice());
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

            if let Some(live_path) = &live_credentials_path_before {
                if live_path.exists() && status.success() {
                    return fs::read(live_path)
                        .with_context(|| format!("could not read {}", live_path.display()));
                }
            }

            let exit_note = if status.success() {
                "Claude exited"
            } else {
                "Claude exited with an error"
            };
            bail!(
                "{} before aisw could capture credentials.\n  \
                 Claude may be storing auth outside the live credential path or in the system keyring.",
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

    let Some(stored_identity) = resolve_profile_oauth_identity(profile_store, name, backend)?
    else {
        return Ok(false);
    };
    let Some(live_identity) = resolve_live_oauth_identity(&snapshot, user_home)? else {
        return Ok(false);
    };

    if stored_identity != live_identity {
        return Ok(false);
    }

    persist_oauth_storage(profile_store, name, backend, &snapshot.bytes)?;
    persist_live_oauth_account_metadata(profile_store, name, user_home)?;
    Ok(true)
}

fn resolve_profile_oauth_identity(
    profile_store: &ProfileStore,
    name: &str,
    backend: CredentialBackend,
) -> Result<Option<String>> {
    // 1. Try credentials file/keychain
    let cred_bytes = match read_stored_credentials(profile_store, name, backend) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    if let Some(id) = identity_from_credentials_json(&cred_bytes)? {
        return Ok(Some(id));
    }

    // 2. Fallback to metadata file
    let Ok(meta_bytes) = profile_store.read_file(Tool::Claude, name, super::OAUTH_ACCOUNT_FILE)
    else {
        return Ok(None);
    };
    identity_from_credentials_json(&meta_bytes)
}

fn resolve_live_oauth_identity(
    snapshot: &LiveCredentialSnapshot,
    user_home: &Path,
) -> Result<Option<String>> {
    // 1. Try live credentials snapshot
    if let Some(id) = identity_from_credentials_json(&snapshot.bytes)? {
        return Ok(Some(id));
    }

    // 2. Fallback to live metadata file
    let Some(metadata) = read_live_oauth_account_metadata_for_import(user_home)? else {
        return Ok(None);
    };
    identity_from_credentials_json(&metadata)
}

pub(super) fn identity_from_credentials_json(bytes: &[u8]) -> Result<Option<String>> {
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .ok()
        .unwrap_or(serde_json::json!({}));

    // Try various paths where email might be stored
    if let Some(email) = v
        .get("account")
        .and_then(|a| a.get("email"))
        .and_then(|e| e.as_str())
    {
        return Ok(Some(email.to_owned()));
    }
    if let Some(email) = v
        .get("oauthAccount")
        .and_then(|a| a.get("emailAddress"))
        .and_then(|e| e.as_str())
    {
        return Ok(Some(email.to_owned()));
    }
    if let Some(email) = v.get("emailAddress").and_then(|e| e.as_str()) {
        return Ok(Some(email.to_owned()));
    }
    Ok(None)
}
