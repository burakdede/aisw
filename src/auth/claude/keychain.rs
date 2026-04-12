//! Auth-storage detection and OS keychain read/write for Claude Code.
//!
//! Claude can store credentials in the OS keychain (macOS Keychain / Linux
//! Secret Service) or in a plain `.credentials.json` file. This module
//! detects which storage is active, reads/writes the keychain entries, and
//! selects the appropriate `CredentialBackend` for newly added profiles.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::CredentialBackend;
use crate::tool_detection;
use crate::types::Tool;

use super::LiveCredentialSource;

// ---- Auth storage detection ----

/// Which storage backend Claude is actively using on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClaudeAuthStorage {
    File,
    Keychain,
}

/// Returns the auth storage Claude is actively reading from on this machine.
pub(super) fn auth_storage(user_home: &Path) -> ClaudeAuthStorage {
    if let Some(storage) = forced_auth_storage() {
        return storage;
    }

    // On macOS, Claude reads credentials from the Keychain even when a
    // credentials file is also present on disk. Prefer Keychain here so that
    // apply_live_credentials updates what Claude actually reads.
    #[cfg(target_os = "macos")]
    if super::super::system_keyring::is_available()
        && read_keychain_credentials().ok().flatten().is_some()
    {
        return ClaudeAuthStorage::Keychain;
    }

    if super::paths::live_credentials_path(user_home).exists() {
        ClaudeAuthStorage::File
    } else if super::super::system_keyring::is_available()
        && read_keychain_credentials().ok().flatten().is_some()
    {
        ClaudeAuthStorage::Keychain
    } else {
        ClaudeAuthStorage::File
    }
}

/// Returns the auth storage override from the test environment, if set.
pub(super) fn forced_auth_storage() -> Option<ClaudeAuthStorage> {
    match super::super::test_overrides::string("AISW_CLAUDE_AUTH_STORAGE").as_deref() {
        Some("file") => Some(ClaudeAuthStorage::File),
        Some("keychain") => Some(ClaudeAuthStorage::Keychain),
        _ => None,
    }
}

// ---- Import support ----

/// Returns `true` when the OS keychain is available for reading Claude's live
/// credentials during profile import.
pub fn keychain_import_supported() -> bool {
    forced_auth_storage() == Some(ClaudeAuthStorage::Keychain)
        || super::super::system_keyring::is_available()
}

/// Returns `true` when the OAuth flow should monitor the keychain for new
/// credentials (rather than only the capture-dir file).
pub(super) fn watch_keychain_during_oauth() -> bool {
    match forced_auth_storage() {
        Some(ClaudeAuthStorage::File) => false,
        Some(ClaudeAuthStorage::Keychain) => true,
        None => super::super::system_keyring::is_available(),
    }
}

// ---- Keychain read/write ----

pub(super) fn keychain_account() -> String {
    super::super::secure_backend::find_generic_password_account(
        super::KEYCHAIN_BACKEND,
        super::KEYCHAIN_SERVICE,
    )
    .ok()
    .flatten()
    .or_else(|| std::env::var("USER").ok())
    .unwrap_or_else(|| "aisw".to_owned())
}

pub(super) fn read_keychain_credentials() -> Result<Option<Vec<u8>>> {
    super::super::secure_backend::read_generic_password(
        super::KEYCHAIN_BACKEND,
        super::KEYCHAIN_SERVICE,
        None,
    )
    .context("could not query the system keyring for Claude Code credentials")
}

/// Reads the live Keychain entry for use during profile import. Returns `None`
/// when the test override forces file-backed storage or keychain is not
/// available.
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

pub(super) fn write_keychain_credentials(bytes: &[u8]) -> Result<()> {
    if cfg!(target_os = "macos")
        && super::super::test_overrides::var("AISW_KEYRING_TEST_DIR").is_none()
    {
        return super::super::macos_keychain::upsert_generic_password(
            super::KEYCHAIN_SERVICE,
            &keychain_account(),
            bytes,
            &trusted_claude_app_paths(),
        )
        .context("could not write Claude Code credentials into the system keyring");
    }

    super::super::secure_backend::upsert_generic_password(
        super::KEYCHAIN_BACKEND,
        super::KEYCHAIN_SERVICE,
        &keychain_account(),
        bytes,
    )
    .context("could not write Claude Code credentials into the system keyring")
}

pub(super) fn delete_keychain_credentials() -> Result<()> {
    super::super::secure_backend::delete_generic_password(
        super::KEYCHAIN_BACKEND,
        super::KEYCHAIN_SERVICE,
        &keychain_account(),
    )
    .context("could not delete Claude Code credentials from the system keyring")
}

pub(super) fn trusted_claude_app_paths() -> Vec<PathBuf> {
    tool_detection::detect(Tool::Claude)
        .map(|detected| vec![detected.binary_path])
        .unwrap_or_default()
}

// ---- Backend selection ----

/// The `CredentialBackend` to use when storing newly captured OAuth credentials.
pub(super) fn oauth_stored_backend() -> CredentialBackend {
    if cfg!(target_os = "macos") {
        return CredentialBackend::File;
    }

    match forced_auth_storage() {
        Some(ClaudeAuthStorage::File) => CredentialBackend::File,
        Some(ClaudeAuthStorage::Keychain) => CredentialBackend::SystemKeyring,
        None => {
            if super::super::system_keyring::is_available()
                && super::super::system_keyring::is_usable()
            {
                CredentialBackend::SystemKeyring
            } else {
                CredentialBackend::File
            }
        }
    }
}

/// Returns the preferred `CredentialBackend` for a profile being imported from
/// the given live credential source.
pub fn preferred_import_backend(source: &LiveCredentialSource) -> CredentialBackend {
    if cfg!(target_os = "macos") && matches!(source, LiveCredentialSource::Keychain) {
        return CredentialBackend::File;
    }

    match source {
        LiveCredentialSource::File(_) => CredentialBackend::File,
        LiveCredentialSource::Keychain => {
            if super::super::system_keyring::is_usable() {
                CredentialBackend::SystemKeyring
            } else {
                CredentialBackend::File
            }
        }
    }
}

/// Returns a human-readable note when the requested backend is SystemKeyring
/// but it is not usable on this machine.
pub fn storage_fallback_note(requested_backend: CredentialBackend) -> Option<String> {
    if requested_backend == CredentialBackend::SystemKeyring
        && !super::super::system_keyring::is_usable()
    {
        return super::super::system_keyring::usability_diagnostic().map(|message| {
            format!(
                "{}\n  aisw will store the managed Claude profile in encrypted local files \
                 instead of the system keyring on this machine.",
                message
            )
        });
    }

    None
}

/// Strips a full `.credentials.json` payload down to the `claudeAiOauth` subset
/// that the Keychain entry should hold.
pub(super) fn live_keychain_payload(credentials: &[u8]) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(credentials) else {
        return credentials.to_vec();
    };
    let Some(claude_ai_oauth) = value.get("claudeAiOauth") else {
        return credentials.to_vec();
    };
    serde_json::to_vec(&serde_json::json!({ "claudeAiOauth": claude_ai_oauth }))
        .unwrap_or_else(|_| credentials.to_vec())
}

/// Returns the backend that a profile should use given its import source.
pub fn imported_profile_backend(source: &LiveCredentialSource) -> CredentialBackend {
    preferred_import_backend(source)
}

/// Returns `true` when Claude is actively reading from the macOS Keychain on
/// this machine (as opposed to the `.credentials.json` file).
pub fn uses_live_keychain(user_home: &Path) -> bool {
    cfg!(target_os = "macos") && matches!(auth_storage(user_home), ClaudeAuthStorage::Keychain)
}
