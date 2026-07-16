//! Auth-storage detection and OS keychain read/write for Claude Code.
//!
//! Claude can store credentials in the OS keychain (macOS Keychain / Linux
//! Secret Service) or in a plain `.credentials.json` file. This module
//! detects which storage is active, reads/writes the keychain entries, and
//! selects the appropriate `CredentialBackend` for newly added profiles.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use semver::Version;
use sha2::{Digest, Sha256};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeKeychainScheme {
    LegacyShared,
    ScopedByConfigDir,
    Unknown,
}

impl ClaudeKeychainScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaudeKeychainScheme::LegacyShared => "legacy_shared",
            ClaudeKeychainScheme::ScopedByConfigDir => "scoped_by_config_dir",
            ClaudeKeychainScheme::Unknown => "unknown",
        }
    }
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

fn shared_keychain_platform() -> bool {
    cfg!(target_os = "macos")
        || matches!(
            super::super::test_overrides::string("AISW_TEST_CLAUDE_PLATFORM").as_deref(),
            Some("macos")
        )
}

fn forced_keychain_scheme() -> Option<ClaudeKeychainScheme> {
    match super::super::test_overrides::string("AISW_CLAUDE_KEYCHAIN_SCHEME").as_deref() {
        Some("legacy_shared") | Some("shared") => Some(ClaudeKeychainScheme::LegacyShared),
        Some("scoped_by_config_dir") | Some("scoped") => {
            Some(ClaudeKeychainScheme::ScopedByConfigDir)
        }
        Some("unknown") => Some(ClaudeKeychainScheme::Unknown),
        _ => None,
    }
}

fn parse_claude_version(raw: &str) -> Option<Version> {
    raw.split_whitespace()
        .find_map(|token| Version::parse(token.trim_start_matches('v')).ok())
}

pub fn detected_keychain_scheme(claude_version: Option<&str>) -> ClaudeKeychainScheme {
    if let Some(forced) = forced_keychain_scheme() {
        return forced;
    }

    let Some(version) = claude_version.and_then(parse_claude_version) else {
        return ClaudeKeychainScheme::Unknown;
    };

    // Evidence from upstream/public tooling indicates older 2.1.19-era builds
    // used one shared service, while 2.1.121+ builds namespace by
    // CLAUDE_CONFIG_DIR. Preserve an explicit unknown band in between.
    if version <= Version::new(2, 1, 19) {
        ClaudeKeychainScheme::LegacyShared
    } else if version >= Version::new(2, 1, 121) {
        ClaudeKeychainScheme::ScopedByConfigDir
    } else {
        ClaudeKeychainScheme::Unknown
    }
}

pub fn current_keychain_scheme() -> ClaudeKeychainScheme {
    let version = tool_detection::detect(Tool::Claude).and_then(|detected| detected.version);
    detected_keychain_scheme(version.as_deref())
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
    read_keychain_credentials_for_service(super::KEYCHAIN_SERVICE)
}

pub(super) fn read_keychain_credentials_for_service(service: &str) -> Result<Option<Vec<u8>>> {
    super::super::secure_backend::read_generic_password(super::KEYCHAIN_BACKEND, service, None)
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
    write_keychain_credentials_for_service(super::KEYCHAIN_SERVICE, bytes)
}

pub(super) fn write_keychain_credentials_for_service(service: &str, bytes: &[u8]) -> Result<()> {
    if cfg!(target_os = "macos")
        && super::super::test_overrides::var("AISW_KEYRING_TEST_DIR").is_none()
    {
        return super::super::macos_keychain::upsert_generic_password(
            service,
            &keychain_account(),
            bytes,
            &trusted_claude_app_paths(),
        )
        .context("could not write Claude Code credentials into the system keyring");
    }

    super::super::secure_backend::upsert_generic_password(
        super::KEYCHAIN_BACKEND,
        service,
        &keychain_account(),
        bytes,
    )
    .context("could not write Claude Code credentials into the system keyring")
}

pub(super) fn delete_keychain_credentials() -> Result<()> {
    delete_keychain_credentials_for_service(super::KEYCHAIN_SERVICE)
}

pub(super) fn delete_keychain_credentials_for_service(service: &str) -> Result<()> {
    super::super::secure_backend::delete_generic_password(
        super::KEYCHAIN_BACKEND,
        service,
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
pub fn oauth_stored_backend() -> CredentialBackend {
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

/// Returns the backend that a profile should use given its import source.
pub fn imported_profile_backend(source: &LiveCredentialSource) -> CredentialBackend {
    preferred_import_backend(source)
}

/// Returns `true` when Claude is actively reading from the macOS Keychain on
/// this machine (as opposed to the `.credentials.json` file).
pub fn uses_live_keychain(user_home: &Path) -> bool {
    shared_keychain_platform() && matches!(auth_storage(user_home), ClaudeAuthStorage::Keychain)
}

pub fn keychain_service_for_config_dir(
    config_dir: &Path,
    user_home: &Path,
    scheme: ClaudeKeychainScheme,
) -> String {
    if !matches!(scheme, ClaudeKeychainScheme::ScopedByConfigDir) {
        return super::KEYCHAIN_SERVICE.to_owned();
    }

    let default_dir = user_home.join(".claude");
    if normalized_path(config_dir) == normalized_path(&default_dir) {
        return super::KEYCHAIN_SERVICE.to_owned();
    }

    let normalized = normalized_path(config_dir);
    let hash = Sha256::digest(normalized.as_os_str().as_encoded_bytes());
    format!(
        "{}-{:02x}{:02x}{:02x}{:02x}",
        super::KEYCHAIN_SERVICE,
        hash[0],
        hash[1],
        hash[2],
        hash[3]
    )
}

fn normalized_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}
