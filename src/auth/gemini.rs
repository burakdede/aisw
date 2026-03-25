use anyhow::{bail, Result};
use chrono::Utc;

use crate::config::{AuthMethod, ConfigStore, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::Tool;

const ENV_FILE: &str = ".env";
const KEY_VAR: &str = "GEMINI_API_KEY";

pub fn add_api_key(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    key: &str,
    label: Option<String>,
) -> Result<()> {
    validate_api_key(key)?;

    profile_store.create(Tool::Gemini, name)?;

    let env_contents = format!("{}={}\n", KEY_VAR, key);
    profile_store
        .write_file(Tool::Gemini, name, ENV_FILE, env_contents.as_bytes())
        .inspect_err(|_| {
            let _ = profile_store.delete(Tool::Gemini, name);
        })?;

    config_store.add_profile(
        Tool::Gemini,
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
        bail!("Gemini API key must not be empty");
    }
    Ok(())
}

/// Read the stored API key from a profile's .env file.
pub fn read_api_key(profile_store: &ProfileStore, name: &str) -> Result<String> {
    let bytes = profile_store.read_file(Tool::Gemini, name, ENV_FILE)?;
    let contents = std::str::from_utf8(&bytes)
        .map_err(|e| anyhow::anyhow!("could not read .env file: {}", e))?;
    for line in contents.lines() {
        if let Some(val) = line.strip_prefix(&format!("{}=", KEY_VAR)) {
            return Ok(val.to_owned());
        }
    }
    anyhow::bail!(".env file missing '{}' entry", KEY_VAR)
}

/// Apply a profile's .env file to `dest` (typically `~/.gemini/.env`).
pub fn apply_env_file(
    profile_store: &ProfileStore,
    name: &str,
    dest: &std::path::Path,
) -> Result<()> {
    let bytes = profile_store.read_file(Tool::Gemini, name, ENV_FILE)?;
    std::fs::write(dest, &bytes)
        .map_err(|e| anyhow::anyhow!("could not write {}: {}", dest.display(), e))?;
    set_permissions_600(dest)
}

#[cfg(unix)]
fn set_permissions_600(path: &std::path::Path) -> Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| anyhow::anyhow!("could not set permissions on {}: {}", path.display(), e))
}

#[cfg(not(unix))]
fn set_permissions_600(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;

    fn valid_key() -> &'static str {
        "AIzaSyTest1234567890"
    }

    fn stores(dir: &std::path::Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
    }

    #[test]
    fn validate_accepts_nonempty_key() {
        assert!(validate_api_key(valid_key()).is_ok());
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_api_key("").is_err());
        assert!(validate_api_key("  ").is_err());
    }

    #[test]
    fn add_creates_env_file() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();

        assert!(ps
            .profile_dir(Tool::Gemini, "default")
            .join(ENV_FILE)
            .exists());
    }

    #[test]
    fn env_file_has_correct_format() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();

        let contents = ps.read_file(Tool::Gemini, "default", ENV_FILE).unwrap();
        let text = std::str::from_utf8(&contents).unwrap();
        assert_eq!(text, format!("GEMINI_API_KEY={}\n", valid_key()));
    }

    #[test]
    fn read_api_key_roundtrip() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();
        assert_eq!(read_api_key(&ps, "default").unwrap(), valid_key());
    }

    #[test]
    fn apply_env_file_writes_to_dest() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();

        let dest_dir = tempdir().unwrap();
        let dest = dest_dir.path().join(".env");
        apply_env_file(&ps, "default", &dest).unwrap();

        let written = std::fs::read_to_string(&dest).unwrap();
        assert!(written.contains(valid_key()));
    }

    #[test]
    fn add_registers_in_config() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), Some("AI Studio".into())).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles.gemini["default"].auth_method,
            AuthMethod::ApiKey
        );
        assert_eq!(
            config.profiles.gemini["default"].label.as_deref(),
            Some("AI Studio")
        );
    }

    #[test]
    #[cfg(unix)]
    fn env_file_has_600_permissions() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();

        let mode = fs::metadata(ps.profile_dir(Tool::Gemini, "default").join(ENV_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn duplicate_profile_errors() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();
        let err = add_api_key(&ps, &cs, "default", valid_key(), None).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn invalid_key_does_not_create_profile() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", "", None).unwrap_err();
        assert!(!ps.exists(Tool::Gemini, "default"));
    }
}
