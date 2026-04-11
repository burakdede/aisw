//! API key add, validate, and read operations for Claude Code profiles.

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::Tool;

use super::super::files;
use super::super::identity;

/// Add a new Claude API-key profile.
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
        profile_store.write_file(
            Tool::Claude,
            name,
            super::CREDENTIALS_FILE,
            credentials.as_bytes(),
        ),
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

/// Validates that the given API key is non-empty.
pub fn validate_api_key(key: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!(
            "Claude API key must not be empty.\n  \
             Get your API key at console.anthropic.com → API Keys.",
        );
    }
    Ok(())
}

/// Reads the stored API key from a profile's credentials file.
pub fn read_api_key(profile_store: &ProfileStore, name: &str) -> Result<String> {
    let bytes = profile_store.read_file(Tool::Claude, name, super::CREDENTIALS_FILE)?;
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
