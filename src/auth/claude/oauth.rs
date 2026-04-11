//! OAuth capture flow and account-metadata persistence for Claude Code.
//!
//! Claude's OAuth flow varies by platform:
//!  - **Non-macOS**: Spawns `claude auth login` with `CLAUDE_CONFIG_DIR` set to
//!    a temporary capture directory and polls for `.credentials.json`.
//!  - **macOS**: Spawns `claude auth login` without the capture dir override
//!    (the override triggers a fallback auth-code flow). Instead, polls the
//!    live keychain and credential file for a change.

use std::fs;
use std::path::{Path, PathBuf};
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
use super::paths::{live_account_metadata_path, live_credentials_path, live_local_state_dir};
use super::{LiveCredentialSnapshot, LiveCredentialSource};

fn oauth_capture_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(super::OAUTH_CAPTURE_DIR)
}

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
/// Spawns `claude auth login` and polls for new credentials. On non-macOS the
/// capture-dir approach is used; on macOS the live locations are polled instead
/// because the override triggers a fallback auth-code flow.
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
    let profile_dir = profile_store.create(Tool::Claude, name)?;
    let stored_backend = oauth_stored_backend();
    let capture_dir = oauth_capture_dir(&profile_dir);
    fs::create_dir_all(&capture_dir)
        .with_context(|| format!("could not create {}", capture_dir.display()))?;

    if let Some(note) = storage_fallback_note(CredentialBackend::SystemKeyring) {
        output::print_warning(note);
    }

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
    output::print_warning(
        "Claude may reuse the account already signed in in your browser. \
If you need a different Claude account, fully sign out of claude.com first, then rerun \
'aisw add claude <name>'.",
    );

    // On macOS, avoid `CLAUDE_CONFIG_DIR` entirely for OAuth login. Claude's
    // browser flow falls back to the auth-code URL shape (`code=true`) when the
    // config dir is overridden, while the normal callback-based flow uses the
    // default live locations. We therefore observe the real live locations on
    // macOS and only use the capture-dir approach on other platforms.
    let use_capture_dir = !cfg!(target_os = "macos");
    let live_credentials_path_before = if cfg!(target_os = "macos") {
        dirs::home_dir().map(|home| live_credentials_path(&home))
    } else {
        None
    };
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
    if use_capture_dir {
        cmd.env("CLAUDE_CONFIG_DIR", capture_dir);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("could not spawn {}", claude_bin.display()))?;

    let credentials_path = capture_dir.join(super::CREDENTIALS_FILE);
    let deadline = Instant::now() + timeout;

    loop {
        if use_capture_dir && credentials_path.exists() {
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
