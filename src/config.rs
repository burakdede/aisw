use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::Tool;

const CURRENT_VERSION: u32 = 1;
const CONFIG_FILE: &str = "config.json";
const AISW_HOME_ENV: &str = "AISW_HOME";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub active: ActiveProfiles,
    pub profiles: AllProfiles,
    pub settings: Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActiveProfiles {
    pub claude: Option<String>,
    pub codex: Option<String>,
    pub gemini: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AllProfiles {
    pub claude: HashMap<String, ProfileMeta>,
    pub codex: HashMap<String, ProfileMeta>,
    pub gemini: HashMap<String, ProfileMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub added_at: DateTime<Utc>,
    pub auth_method: AuthMethod,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    OAuth,
    ApiKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub backup_on_switch: bool,
    pub max_backups: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            active: ActiveProfiles::default(),
            profiles: AllProfiles::default(),
            settings: Settings::default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backup_on_switch: true,
            max_backups: 10,
        }
    }
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(home: &Path) -> Self {
        Self {
            path: home.join(CONFIG_FILE),
        }
    }

    /// Resolve the aisw home directory from AISW_HOME env var or ~/.aisw/.
    pub fn aisw_home() -> Result<PathBuf> {
        if let Ok(val) = std::env::var(AISW_HOME_ENV) {
            return Ok(PathBuf::from(val));
        }
        let home = dirs::home_dir().context("could not determine home directory")?;
        Ok(home.join(".aisw"))
    }

    pub fn load(&self) -> Result<Config> {
        if !self.path.exists() {
            let config = Config::default();
            self.save(&config)?;
            return Ok(config);
        }

        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("could not read {}", self.path.display()))?;

        let config: Config = serde_json::from_str(&contents)
            .with_context(|| format!("could not parse {}", self.path.display()))?;

        if config.version > CURRENT_VERSION {
            bail!(
                "config version {} is newer than this version of aisw supports (max: {}).\n  \
                 Upgrade aisw to continue: https://github.com/burakdede/aisw#install",
                config.version,
                CURRENT_VERSION
            );
        }

        Ok(config)
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create directory {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(config).context("could not serialize config")?;

        // Write to a temp file alongside the target, then rename atomically.
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, &json).with_context(|| format!("could not write {}", tmp.display()))?;

        set_file_permissions_600(&tmp)?;

        fs::rename(&tmp, &self.path).with_context(|| {
            format!(
                "could not move config into place at {}",
                self.path.display()
            )
        })?;

        Ok(())
    }

    pub fn add_profile(&self, tool: Tool, name: &str, meta: ProfileMeta) -> Result<Config> {
        let mut config = self.load()?;
        let profiles = tool_profiles_mut(&mut config, tool);

        if profiles.contains_key(name) {
            bail!(
                "profile '{}' already exists for {}.\n  \
                 Use 'aisw list {}' to see existing profiles.",
                name,
                tool,
                tool
            );
        }

        profiles.insert(name.to_owned(), meta);
        self.save(&config)?;
        Ok(config)
    }

    pub fn upsert_profile(&self, tool: Tool, name: &str, meta: ProfileMeta) -> Result<Config> {
        let mut config = self.load()?;
        tool_profiles_mut(&mut config, tool).insert(name.to_owned(), meta);
        self.save(&config)?;
        Ok(config)
    }

    pub fn remove_profile(&self, tool: Tool, name: &str) -> Result<Config> {
        let mut config = self.load()?;
        let profiles = tool_profiles_mut(&mut config, tool);

        if profiles.remove(name).is_none() {
            bail!(
                "profile '{}' not found for {}.\n  \
                 Run 'aisw list {}' to see available profiles.",
                name,
                tool,
                tool
            );
        }

        self.save(&config)?;
        Ok(config)
    }

    pub fn rename_profile(&self, tool: Tool, old_name: &str, new_name: &str) -> Result<Config> {
        let mut config = self.load()?;
        let profiles = tool_profiles_mut(&mut config, tool);

        if old_name == new_name {
            bail!("profile '{}' is already named '{}'.", old_name, new_name);
        }

        let meta = profiles.remove(old_name).ok_or_else(|| {
            anyhow::anyhow!(
                "profile '{}' not found for {}.\n  \
                 Run 'aisw list {}' to see available profiles.",
                old_name,
                tool,
                tool
            )
        })?;

        if profiles.contains_key(new_name) {
            profiles.insert(old_name.to_owned(), meta);
            bail!(
                "profile '{}' already exists for {}.\n  \
                 Use 'aisw list {}' to see existing profiles.",
                new_name,
                tool,
                tool
            );
        }

        profiles.insert(new_name.to_owned(), meta);

        if tool_active(&config, tool).as_deref() == Some(old_name) {
            *tool_active_mut(&mut config, tool) = Some(new_name.to_owned());
        }

        self.save(&config)?;
        Ok(config)
    }

    pub fn set_active(&self, tool: Tool, name: &str) -> Result<Config> {
        let mut config = self.load()?;

        if !tool_profiles(&config, tool).contains_key(name) {
            bail!(
                "profile '{}' not found for {}.\n  \
                 Run 'aisw list {}' to see available profiles.",
                name,
                tool,
                tool
            );
        }

        *tool_active_mut(&mut config, tool) = Some(name.to_owned());
        self.save(&config)?;
        Ok(config)
    }

    pub fn clear_active(&self, tool: Tool) -> Result<Config> {
        let mut config = self.load()?;
        *tool_active_mut(&mut config, tool) = None;
        self.save(&config)?;
        Ok(config)
    }

    pub fn get_active<'c>(&self, config: &'c Config, tool: Tool) -> Option<&'c str> {
        tool_active(config, tool).as_deref()
    }
}

fn tool_profiles(config: &Config, tool: Tool) -> &HashMap<String, ProfileMeta> {
    match tool {
        Tool::Claude => &config.profiles.claude,
        Tool::Codex => &config.profiles.codex,
        Tool::Gemini => &config.profiles.gemini,
    }
}

fn tool_profiles_mut(config: &mut Config, tool: Tool) -> &mut HashMap<String, ProfileMeta> {
    match tool {
        Tool::Claude => &mut config.profiles.claude,
        Tool::Codex => &mut config.profiles.codex,
        Tool::Gemini => &mut config.profiles.gemini,
    }
}

fn tool_active(config: &Config, tool: Tool) -> &Option<String> {
    match tool {
        Tool::Claude => &config.active.claude,
        Tool::Codex => &config.active.codex,
        Tool::Gemini => &config.active.gemini,
    }
}

fn tool_active_mut(config: &mut Config, tool: Tool) -> &mut Option<String> {
    match tool {
        Tool::Claude => &mut config.active.claude,
        Tool::Codex => &mut config.active.codex,
        Tool::Gemini => &mut config.active.gemini,
    }
}

#[cfg(unix)]
fn set_file_permissions_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_file_permissions_600(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store(dir: &Path) -> ConfigStore {
        ConfigStore::new(dir)
    }

    fn meta(method: AuthMethod) -> ProfileMeta {
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: method,
            label: None,
        }
    }

    #[test]
    fn default_config_is_valid_v1() {
        let config = Config::default();
        assert_eq!(config.version, 1);
        let json = serde_json::to_string(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
    }

    #[test]
    fn load_creates_file_when_absent() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());
        assert!(!store.path.exists());

        let config = store.load().unwrap();
        assert_eq!(config.version, 1);
        assert!(store.path.exists());
    }

    #[test]
    fn save_is_atomic_and_sets_permissions() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());
        let config = Config::default();

        store.save(&config).unwrap();

        assert!(store.path.exists());
        // Temp file should be gone
        assert!(!store.path.with_extension("json.tmp").exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&store.path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "config.json must be 0600");
        }
    }

    #[test]
    fn add_profile_and_retrieve() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(Tool::Claude, "work", meta(AuthMethod::OAuth))
            .unwrap();
        let config = store.load().unwrap();

        assert!(config.profiles.claude.contains_key("work"));
        assert_eq!(
            config.profiles.claude["work"].auth_method,
            AuthMethod::OAuth
        );
    }

    #[test]
    fn duplicate_profile_is_rejected() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(Tool::Claude, "work", meta(AuthMethod::OAuth))
            .unwrap();
        let err = store
            .add_profile(Tool::Claude, "work", meta(AuthMethod::ApiKey))
            .unwrap_err();

        assert!(err.to_string().contains("already exists"));
        assert!(err.to_string().contains("work"));
    }

    #[test]
    fn remove_profile() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(Tool::Codex, "personal", meta(AuthMethod::ApiKey))
            .unwrap();
        store.remove_profile(Tool::Codex, "personal").unwrap();

        let config = store.load().unwrap();
        assert!(!config.profiles.codex.contains_key("personal"));
    }

    #[test]
    fn remove_nonexistent_profile_errors() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        let err = store.remove_profile(Tool::Gemini, "ghost").unwrap_err();
        assert!(err.to_string().contains("not found"));
        assert!(err.to_string().contains("ghost"));
    }

    #[test]
    fn upsert_profile_recreates_missing_entry() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .upsert_profile(Tool::Claude, "work", meta(AuthMethod::ApiKey))
            .unwrap();

        let config = store.load().unwrap();
        assert_eq!(
            config.profiles.claude["work"].auth_method,
            AuthMethod::ApiKey
        );
    }

    #[test]
    fn upsert_profile_overwrites_existing_entry() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(Tool::Claude, "work", meta(AuthMethod::OAuth))
            .unwrap();
        store
            .upsert_profile(Tool::Claude, "work", meta(AuthMethod::ApiKey))
            .unwrap();

        let config = store.load().unwrap();
        assert_eq!(
            config.profiles.claude["work"].auth_method,
            AuthMethod::ApiKey
        );
    }

    #[test]
    fn set_active_and_get_active() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(Tool::Claude, "work", meta(AuthMethod::OAuth))
            .unwrap();
        store.set_active(Tool::Claude, "work").unwrap();

        let config = store.load().unwrap();
        assert_eq!(store.get_active(&config, Tool::Claude), Some("work"));
        assert_eq!(store.get_active(&config, Tool::Codex), None);
    }

    #[test]
    fn rename_profile_updates_entry_and_active_reference() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(Tool::Claude, "default", meta(AuthMethod::OAuth))
            .unwrap();
        store.set_active(Tool::Claude, "default").unwrap();
        store
            .rename_profile(Tool::Claude, "default", "work")
            .unwrap();

        let config = store.load().unwrap();
        assert!(!config.profiles.claude.contains_key("default"));
        assert!(config.profiles.claude.contains_key("work"));
        assert_eq!(store.get_active(&config, Tool::Claude), Some("work"));
    }

    #[test]
    fn rename_profile_rejects_duplicate_target() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(Tool::Claude, "default", meta(AuthMethod::OAuth))
            .unwrap();
        store
            .add_profile(Tool::Claude, "work", meta(AuthMethod::ApiKey))
            .unwrap();

        let err = store
            .rename_profile(Tool::Claude, "default", "work")
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn set_active_nonexistent_errors() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        let err = store.set_active(Tool::Claude, "ghost").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn clear_active() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(Tool::Gemini, "default", meta(AuthMethod::ApiKey))
            .unwrap();
        store.set_active(Tool::Gemini, "default").unwrap();
        store.clear_active(Tool::Gemini).unwrap();

        let config = store.load().unwrap();
        assert_eq!(store.get_active(&config, Tool::Gemini), None);
    }

    #[test]
    fn future_version_is_rejected() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        // Write a config with a version higher than supported
        let future = serde_json::json!({
            "version": 99,
            "active": { "claude": null, "codex": null, "gemini": null },
            "profiles": { "claude": {}, "codex": {}, "gemini": {} },
            "settings": { "backup_on_switch": true, "max_backups": 10 }
        });
        fs::write(&store.path, future.to_string()).unwrap();

        let err = store.load().unwrap_err();
        assert!(err.to_string().contains("99"));
        assert!(err.to_string().contains("Upgrade"));
    }

    #[test]
    fn round_trip_preserves_data() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .add_profile(
                Tool::Claude,
                "work",
                ProfileMeta {
                    added_at: Utc::now(),
                    auth_method: AuthMethod::OAuth,
                    label: Some("Work subscription".to_owned()),
                },
            )
            .unwrap();
        store.set_active(Tool::Claude, "work").unwrap();

        let config = store.load().unwrap();
        assert_eq!(
            config.profiles.claude["work"].label.as_deref(),
            Some("Work subscription")
        );
        assert_eq!(config.active.claude.as_deref(), Some("work"));
    }
}
