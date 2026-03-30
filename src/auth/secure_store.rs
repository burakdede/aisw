use anyhow::{bail, Result};

use super::macos_keychain;
use crate::types::Tool;

const SERVICE: &str = "aisw";

pub fn read_profile_secret(tool: Tool, profile_name: &str) -> Result<Option<Vec<u8>>> {
    macos_keychain::read_generic_password(SERVICE, Some(&profile_account(tool, profile_name)))
}

pub fn write_profile_secret(tool: Tool, profile_name: &str, bytes: &[u8]) -> Result<()> {
    macos_keychain::upsert_generic_password(SERVICE, &profile_account(tool, profile_name), bytes)
}

pub fn delete_profile_secret(tool: Tool, profile_name: &str) -> Result<()> {
    macos_keychain::delete_generic_password(SERVICE, &profile_account(tool, profile_name))
}

pub fn rename_profile_secret(tool: Tool, old_name: &str, new_name: &str) -> Result<()> {
    let Some(bytes) = read_profile_secret(tool, old_name)? else {
        bail!(
            "secure credentials for {} profile '{}' are missing from macOS Keychain",
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
            "secure credentials for {} profile '{}' are missing from macOS Keychain",
            tool,
            profile_name
        );
    };
    macos_keychain::upsert_generic_password(
        SERVICE,
        &backup_account(tool, profile_name, backup_id),
        &bytes,
    )
}

pub fn restore_profile_secret(tool: Tool, profile_name: &str, backup_id: &str) -> Result<()> {
    let Some(bytes) = macos_keychain::read_generic_password(
        SERVICE,
        Some(&backup_account(tool, profile_name, backup_id)),
    )?
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
    macos_keychain::delete_generic_password(SERVICE, &backup_account(tool, profile_name, backup_id))
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
