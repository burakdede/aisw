use anyhow::{bail, Result};

use super::secure_backend::{self, SecureBackend};
use crate::types::Tool;

const SERVICE: &str = "aisw";
const BACKEND: SecureBackend = SecureBackend::SystemKeyring;

fn enrich_system_keyring_error(err: anyhow::Error) -> anyhow::Error {
    if let Some(diagnostic) = super::system_keyring::usability_diagnostic() {
        return err.context(diagnostic);
    }
    err
}

pub fn read_profile_secret(tool: Tool, profile_name: &str) -> Result<Option<Vec<u8>>> {
    secure_backend::read_generic_password(
        BACKEND,
        SERVICE,
        Some(&profile_account(tool, profile_name)),
    )
    .map_err(enrich_system_keyring_error)
}

pub fn write_profile_secret(tool: Tool, profile_name: &str, bytes: &[u8]) -> Result<()> {
    secure_backend::upsert_generic_password(
        BACKEND,
        SERVICE,
        &profile_account(tool, profile_name),
        bytes,
    )
    .map_err(enrich_system_keyring_error)
}

pub fn delete_profile_secret(tool: Tool, profile_name: &str) -> Result<()> {
    secure_backend::delete_generic_password(BACKEND, SERVICE, &profile_account(tool, profile_name))
        .map_err(enrich_system_keyring_error)
}

pub fn rename_profile_secret(tool: Tool, old_name: &str, new_name: &str) -> Result<()> {
    let Some(bytes) = read_profile_secret(tool, old_name)? else {
        bail!(
            "secure credentials for {} profile '{}' are missing from the system keyring",
            tool,
            old_name
        );
    };
    write_profile_secret(tool, new_name, &bytes)?;
    delete_profile_secret(tool, old_name)
}

pub fn snapshot_profile_secret(tool: Tool, profile_name: &str, backup_id: &str) -> Result<()> {
    let Some(bytes) = read_profile_secret(tool, profile_name)? else {
        bail!(
            "secure credentials for {} profile '{}' are missing from the system keyring",
            tool,
            profile_name
        );
    };
    secure_backend::upsert_generic_password(
        BACKEND,
        SERVICE,
        &backup_account(tool, profile_name, backup_id),
        &bytes,
    )
    .map_err(enrich_system_keyring_error)
}

pub fn restore_profile_secret(tool: Tool, profile_name: &str, backup_id: &str) -> Result<()> {
    let Some(bytes) = secure_backend::read_generic_password(
        BACKEND,
        SERVICE,
        Some(&backup_account(tool, profile_name, backup_id)),
    )
    .map_err(enrich_system_keyring_error)?
    else {
        bail!(
            "backup '{}' is missing secure credentials for {} profile '{}'",
            backup_id,
            tool,
            profile_name
        );
    };
    write_profile_secret(tool, profile_name, &bytes)
}

pub fn delete_backup_secret(tool: Tool, profile_name: &str, backup_id: &str) -> Result<()> {
    secure_backend::delete_generic_password(
        BACKEND,
        SERVICE,
        &backup_account(tool, profile_name, backup_id),
    )
    .map_err(enrich_system_keyring_error)
}

fn profile_account(tool: Tool, profile_name: &str) -> String {
    format!("profile:{}:{}", tool.binary_name(), profile_name)
}

fn backup_account(tool: Tool, profile_name: &str, backup_id: &str) -> String {
    format!(
        "backup:{}:{}:{}",
        backup_id,
        tool.binary_name(),
        profile_name
    )
}
