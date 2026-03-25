use anyhow::{bail, Result};
use chrono::Utc;

use crate::config::{AuthMethod, ConfigStore, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::Tool;

const AUTH_FILE: &str = "auth.json";
const CONFIG_FILE: &str = "config.toml";

// Codex reads credentials from a file rather than the OS keyring when this is set.
const CONFIG_TOML_CONTENTS: &str = "cli_auth_credentials_store = \"file\"\n";

pub fn add_api_key(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    key: &str,
    label: Option<String>,
) -> Result<()> {
    validate_api_key(key)?;

    profile_store.create(Tool::Codex, name)?;

    let cleanup = |ps: &ProfileStore| {
        let _ = ps.delete(Tool::Codex, name);
    };

    profile_store
        .write_file(
            Tool::Codex,
            name,
            CONFIG_FILE,
            CONFIG_TOML_CONTENTS.as_bytes(),
        )
        .inspect_err(|_| cleanup(profile_store))?;

    let auth_json = format!("{{\"token\":\"{}\"}}", key);
    profile_store
        .write_file(Tool::Codex, name, AUTH_FILE, auth_json.as_bytes())
        .inspect_err(|_| cleanup(profile_store))?;

    config_store.add_profile(
        Tool::Codex,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::ApiKey,
            label,
        },
    )?;

    Ok(())
}

pub fn validate_api_key(key: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!("Codex API key must not be empty");
    }
    Ok(())
}

/// Read the stored API token from a profile's auth file.
pub fn read_api_key(profile_store: &ProfileStore, name: &str) -> Result<String> {
    let bytes = profile_store.read_file(Tool::Codex, name, AUTH_FILE)?;
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("could not parse auth file: {}", e))?;
    json["token"]
        .as_str()
        .map(|s| s.to_owned())
        .ok_or_else(|| anyhow::anyhow!("auth file missing 'token' field"))
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
            config.profiles.codex["main"].auth_method,
            AuthMethod::ApiKey
        );
        assert_eq!(config.profiles.codex["main"].label.as_deref(), Some("Work"));
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
}
