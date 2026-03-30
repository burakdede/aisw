use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::types::{StateMode, Tool};

const CURRENT_VERSION: u32 = 1;
const CONFIG_FILE: &str = "config.json";
const CONFIG_LOCK_FILE: &str = "config.json.lock";
const AISW_HOME_ENV: &str = "AISW_HOME";
const CONFIG_LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(25);
const CONFIG_LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub active: ActiveProfiles,
    pub profiles: AllProfiles,
    pub settings: Settings,
}

#[derive(Debug, Clone, Default)]
pub struct ActiveProfiles(HashMap<Tool, Option<String>>);

#[derive(Debug, Clone, Default)]
pub struct AllProfiles(HashMap<Tool, HashMap<String, ProfileMeta>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub added_at: DateTime<Utc>,
    pub auth_method: AuthMethod,
    #[serde(default)]
    pub credential_backend: CredentialBackend,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    OAuth,
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CredentialBackend {
    #[default]
    File,
    MacosKeychain,
}

impl CredentialBackend {
    pub fn display_name(self) -> &'static str {
        match self {
            CredentialBackend::File => "file",
            CredentialBackend::MacosKeychain => "macos_keychain",
        }
    }

    pub fn validate_for_tool(self, tool: Tool) -> Result<()> {
        match (self, tool) {
            (CredentialBackend::File, _) => Ok(()),
            (CredentialBackend::MacosKeychain, Tool::Claude | Tool::Codex) => Ok(()),
            (CredentialBackend::MacosKeychain, Tool::Gemini) => bail!(
                "credential backend '{}' is not supported for {}.\n  \
                 Gemini CLI auth remains file-managed because its local ~/.gemini state mixes \
                 credentials with broader tool state.",
                self.display_name(),
                tool
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub backup_on_switch: bool,
    pub max_backups: usize,
    pub tool_settings: HashMap<Tool, ToolSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolSettings {
    #[serde(default)]
    pub state_mode: StateMode,
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
        let mut tool_settings = HashMap::new();
        for tool in Tool::ALL {
            if tool.supports_state_mode() {
                tool_settings.insert(tool, ToolSettings::default());
            }
        }

        Self {
            backup_on_switch: true,
            max_backups: 10,
            tool_settings,
        }
    }
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            state_mode: StateMode::Isolated,
        }
    }
}

impl Config {
    pub fn profiles_for(&self, tool: Tool) -> &HashMap<String, ProfileMeta> {
        tool_profiles(self, tool)
    }

    pub fn active_for(&self, tool: Tool) -> Option<&str> {
        tool_active(self, tool).as_deref()
    }

    pub fn state_mode_for(&self, tool: Tool) -> StateMode {
        self.settings.state_mode(tool)
    }
}

pub struct ConfigStore {
    path: PathBuf,
    lock_path: PathBuf,
}

impl ConfigStore {
    pub fn new(home: &Path) -> Self {
        Self {
            path: home.join(CONFIG_FILE),
            lock_path: home.join(CONFIG_LOCK_FILE),
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
        if self.path.exists() {
            return self.load_existing();
        }

        let _lock = self.acquire_lock()?;
        if self.path.exists() {
            return self.load_existing();
        }

        let config = Config::default();
        self.save_unlocked(&config)?;
        Ok(config)
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        let _lock = self.acquire_lock()?;
        self.save_unlocked(config)
    }

    pub fn add_profile(&self, tool: Tool, name: &str, meta: ProfileMeta) -> Result<Config> {
        self.with_mutating_config(|config| {
            let profiles = tool_profiles_mut(config, tool);

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
            Ok(())
        })
    }

    pub fn upsert_profile(&self, tool: Tool, name: &str, meta: ProfileMeta) -> Result<Config> {
        self.with_mutating_config(|config| {
            tool_profiles_mut(config, tool).insert(name.to_owned(), meta);
            Ok(())
        })
    }

    pub fn remove_profile(&self, tool: Tool, name: &str) -> Result<Config> {
        self.with_mutating_config(|config| {
            let profiles = tool_profiles_mut(config, tool);

            if profiles.remove(name).is_none() {
                bail!(
                    "profile '{}' not found for {}.\n  \
                     Run 'aisw list {}' to see available profiles.",
                    name,
                    tool,
                    tool
                );
            }

            Ok(())
        })
    }

    pub fn rename_profile(&self, tool: Tool, old_name: &str, new_name: &str) -> Result<Config> {
        self.with_mutating_config(|config| {
            let profiles = tool_profiles_mut(config, tool);

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

            if tool_active(config, tool).as_deref() == Some(old_name) {
                *tool_active_mut(config, tool) = Some(new_name.to_owned());
            }

            Ok(())
        })
    }

    pub fn set_active(&self, tool: Tool, name: &str) -> Result<Config> {
        self.with_mutating_config(|config| {
            if !tool_profiles(config, tool).contains_key(name) {
                bail!(
                    "profile '{}' not found for {}.\n  \
                     Run 'aisw list {}' to see available profiles.",
                    name,
                    tool,
                    tool
                );
            }

            *tool_active_mut(config, tool) = Some(name.to_owned());
            Ok(())
        })
    }

    pub fn activate_profile(
        &self,
        tool: Tool,
        name: &str,
        state_mode: Option<StateMode>,
    ) -> Result<Config> {
        self.with_mutating_config(|config| {
            if !tool_profiles(config, tool).contains_key(name) {
                bail!(
                    "profile '{}' not found for {}.\n  \
                     Run 'aisw list {}' to see available profiles.",
                    name,
                    tool,
                    tool
                );
            }

            *tool_active_mut(config, tool) = Some(name.to_owned());
            if let Some(mode) = state_mode {
                *tool_state_mode_mut(config, tool) = mode;
            }
            Ok(())
        })
    }

    pub fn clear_active(&self, tool: Tool) -> Result<Config> {
        self.with_mutating_config(|config| {
            *tool_active_mut(config, tool) = None;
            Ok(())
        })
    }

    pub fn set_state_mode(&self, tool: Tool, mode: StateMode) -> Result<Config> {
        self.with_mutating_config(|config| {
            *tool_state_mode_mut(config, tool) = mode;
            Ok(())
        })
    }

    pub fn get_active<'c>(&self, config: &'c Config, tool: Tool) -> Option<&'c str> {
        tool_active(config, tool).as_deref()
    }

    fn with_mutating_config<F>(&self, mutate: F) -> Result<Config>
    where
        F: FnOnce(&mut Config) -> Result<()>,
    {
        let _lock = self.acquire_lock()?;
        let mut config = self.load_unlocked()?;
        mutate(&mut config)?;
        self.save_unlocked(&config)?;
        Ok(config)
    }

    #[cfg(test)]
    fn with_mutating_config_timeout_for_test<F>(
        &self,
        timeout: Duration,
        mutate: F,
    ) -> Result<Config>
    where
        F: FnOnce(&mut Config) -> Result<()>,
    {
        let _lock = self.acquire_lock_with_timeout(timeout)?;
        let mut config = self.load_unlocked()?;
        mutate(&mut config)?;
        self.save_unlocked(&config)?;
        Ok(config)
    }

    fn load_existing(&self) -> Result<Config> {
        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("could not read {}", self.path.display()))?;

        let config: Config = serde_json::from_str(&contents)
            .with_context(|| format!("could not parse {}", self.path.display()))?;

        validate_config_version(&config)?;
        Ok(config)
    }

    fn load_unlocked(&self) -> Result<Config> {
        if !self.path.exists() {
            let config = Config::default();
            self.save_unlocked(&config)?;
            return Ok(config);
        }

        self.load_existing()
    }

    fn save_unlocked(&self, config: &Config) -> Result<()> {
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

    fn acquire_lock(&self) -> Result<ConfigLockGuard> {
        self.acquire_lock_with_timeout(CONFIG_LOCK_WAIT_TIMEOUT)
    }

    fn acquire_lock_with_timeout(&self, timeout: Duration) -> Result<ConfigLockGuard> {
        if let Some(parent) = self.lock_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create directory {}", parent.display()))?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)
            .with_context(|| format!("could not open {}", self.lock_path.display()))?;

        set_file_permissions_600(&self.lock_path)?;

        let started_at = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(ConfigLockGuard { file }),
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    if started_at.elapsed() >= timeout {
                        bail!(
                            "timed out waiting for config lock at {}.\n  \
                             Another aisw command is updating configuration. Wait for it to \
                             finish, then retry.",
                            self.lock_path.display()
                        );
                    }
                    thread::sleep(CONFIG_LOCK_RETRY_INTERVAL);
                }
                Err(err) => {
                    return Err(err)
                        .with_context(|| format!("could not lock {}", self.lock_path.display()));
                }
            }
        }
    }
}

impl ActiveProfiles {
    pub fn get(&self, tool: Tool) -> Option<&Option<String>> {
        self.0.get(&tool)
    }

    pub fn get_mut(&mut self, tool: Tool) -> &mut Option<String> {
        self.0.entry(tool).or_default()
    }
}

impl AllProfiles {
    pub fn get(&self, tool: Tool) -> Option<&HashMap<String, ProfileMeta>> {
        self.0.get(&tool)
    }

    pub fn get_mut(&mut self, tool: Tool) -> &mut HashMap<String, ProfileMeta> {
        self.0.entry(tool).or_default()
    }
}

impl Settings {
    pub fn state_mode(&self, tool: Tool) -> StateMode {
        self.tool_settings
            .get(&tool)
            .map(|settings| settings.state_mode)
            .unwrap_or(StateMode::Isolated)
    }

    pub fn state_mode_mut(&mut self, tool: Tool) -> &mut StateMode {
        assert!(
            tool.supports_state_mode(),
            "{tool} does not support configurable state mode"
        );
        &mut self.tool_settings.entry(tool).or_default().state_mode
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LegacyActiveProfiles {
    pub claude: Option<String>,
    pub codex: Option<String>,
    pub gemini: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LegacyAllProfiles {
    pub claude: HashMap<String, ProfileMeta>,
    pub codex: HashMap<String, ProfileMeta>,
    pub gemini: HashMap<String, ProfileMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LegacySettings {
    pub backup_on_switch: bool,
    pub max_backups: usize,
    #[serde(default)]
    pub claude: ToolSettings,
    #[serde(default)]
    pub codex: ToolSettings,
}

impl Serialize for ActiveProfiles {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        LegacyActiveProfiles {
            claude: self.get(Tool::Claude).cloned().flatten(),
            codex: self.get(Tool::Codex).cloned().flatten(),
            gemini: self.get(Tool::Gemini).cloned().flatten(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ActiveProfiles {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let legacy = LegacyActiveProfiles::deserialize(deserializer)?;
        let mut inner = HashMap::new();
        inner.insert(Tool::Claude, legacy.claude);
        inner.insert(Tool::Codex, legacy.codex);
        inner.insert(Tool::Gemini, legacy.gemini);
        Ok(Self(inner))
    }
}

impl Serialize for AllProfiles {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        LegacyAllProfiles {
            claude: self.get(Tool::Claude).cloned().unwrap_or_default(),
            codex: self.get(Tool::Codex).cloned().unwrap_or_default(),
            gemini: self.get(Tool::Gemini).cloned().unwrap_or_default(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AllProfiles {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let legacy = LegacyAllProfiles::deserialize(deserializer)?;
        let mut inner = HashMap::new();
        inner.insert(Tool::Claude, legacy.claude);
        inner.insert(Tool::Codex, legacy.codex);
        inner.insert(Tool::Gemini, legacy.gemini);
        Ok(Self(inner))
    }
}

impl Serialize for Settings {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        LegacySettings {
            backup_on_switch: self.backup_on_switch,
            max_backups: self.max_backups,
            claude: self
                .tool_settings
                .get(&Tool::Claude)
                .cloned()
                .unwrap_or_default(),
            codex: self
                .tool_settings
                .get(&Tool::Codex)
                .cloned()
                .unwrap_or_default(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let legacy = LegacySettings::deserialize(deserializer)?;
        let mut tool_settings = HashMap::new();
        tool_settings.insert(Tool::Claude, legacy.claude);
        tool_settings.insert(Tool::Codex, legacy.codex);

        Ok(Self {
            backup_on_switch: legacy.backup_on_switch,
            max_backups: legacy.max_backups,
            tool_settings,
        })
    }
}

struct ConfigLockGuard {
    file: fs::File,
}

impl Drop for ConfigLockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn validate_config_version(config: &Config) -> Result<()> {
    if config.version > CURRENT_VERSION {
        bail!(
            "config version {} is newer than this version of aisw supports (max: {}).\n  \
             Upgrade aisw to continue: https://github.com/burakdede/aisw#install",
            config.version,
            CURRENT_VERSION
        );
    }

    Ok(())
}

fn tool_profiles(config: &Config, tool: Tool) -> &HashMap<String, ProfileMeta> {
    config.profiles.get(tool).unwrap_or(&EMPTY_PROFILES)
}

fn tool_profiles_mut(config: &mut Config, tool: Tool) -> &mut HashMap<String, ProfileMeta> {
    config.profiles.get_mut(tool)
}

fn tool_active(config: &Config, tool: Tool) -> &Option<String> {
    config.active.get(tool).unwrap_or(&EMPTY_ACTIVE)
}

fn tool_active_mut(config: &mut Config, tool: Tool) -> &mut Option<String> {
    config.active.get_mut(tool)
}

fn tool_state_mode_mut(config: &mut Config, tool: Tool) -> &mut StateMode {
    config.settings.state_mode_mut(tool)
}

static EMPTY_ACTIVE: Option<String> = None;
static EMPTY_PROFILES: std::sync::LazyLock<HashMap<String, ProfileMeta>> =
    std::sync::LazyLock::new(HashMap::new);

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
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    use tempfile::tempdir;

    fn store(dir: &Path) -> ConfigStore {
        ConfigStore::new(dir)
    }

    fn meta(method: AuthMethod) -> ProfileMeta {
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: method,
            credential_backend: CredentialBackend::File,
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
    fn load_defaults_missing_codex_settings_to_isolated() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());
        fs::write(
            dir.path().join(CONFIG_FILE),
            r#"{
  "version": 1,
  "active": {"claude": null, "codex": null, "gemini": null},
  "profiles": {"claude": {}, "codex": {}, "gemini": {}},
  "settings": {"backup_on_switch": true, "max_backups": 10}
}"#,
        )
        .unwrap();

        let config = store.load().unwrap();
        assert_eq!(config.state_mode_for(Tool::Claude), StateMode::Isolated);
        assert_eq!(config.state_mode_for(Tool::Codex), StateMode::Isolated);
    }

    #[test]
    fn load_defaults_missing_credential_backend_to_file() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());
        fs::write(
            dir.path().join(CONFIG_FILE),
            r#"{
  "version": 1,
  "active": {"claude": "work", "codex": null, "gemini": null},
  "profiles": {
    "claude": {
      "work": {
        "added_at": "2026-03-30T00:00:00Z",
        "auth_method": "o_auth",
        "label": "legacy"
      }
    },
    "codex": {},
    "gemini": {}
  },
  "settings": {"backup_on_switch": true, "max_backups": 10}
}"#,
        )
        .unwrap();

        let config = store.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].credential_backend,
            CredentialBackend::File
        );
    }

    #[test]
    fn macos_keychain_backend_is_rejected_for_gemini() {
        let err = CredentialBackend::MacosKeychain
            .validate_for_tool(Tool::Gemini)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("credential backend 'macos_keychain' is not supported for gemini"));
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

        assert!(config.profiles_for(Tool::Claude).contains_key("work"));
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].auth_method,
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
    fn set_state_mode_persists() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        store
            .set_state_mode(Tool::Codex, StateMode::Shared)
            .unwrap();

        let config = store.load().unwrap();
        assert_eq!(config.state_mode_for(Tool::Codex), StateMode::Shared);
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
        assert!(!config.profiles_for(Tool::Codex).contains_key("personal"));
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
            config.profiles_for(Tool::Claude)["work"].auth_method,
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
            config.profiles_for(Tool::Claude)["work"].auth_method,
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
        assert!(!config.profiles_for(Tool::Claude).contains_key("default"));
        assert!(config.profiles_for(Tool::Claude).contains_key("work"));
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
                    credential_backend: CredentialBackend::File,
                    label: Some("Work subscription".to_owned()),
                },
            )
            .unwrap();
        store.set_active(Tool::Claude, "work").unwrap();

        let config = store.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].label.as_deref(),
            Some("Work subscription")
        );
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].credential_backend,
            CredentialBackend::File
        );
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
    }

    #[test]
    fn concurrent_process_writers_do_not_lose_updates() {
        let dir = tempdir().unwrap();
        let signal_path = dir.path().join("lock-held.signal");
        let mut child = spawn_lock_helper(dir.path(), &signal_path, 450, "child");

        wait_for_file(&signal_path);

        let started_at = Instant::now();
        store(dir.path())
            .add_profile(Tool::Claude, "parent", meta(AuthMethod::ApiKey))
            .unwrap();
        let elapsed = started_at.elapsed();

        let status = child.wait().unwrap();
        assert!(status.success(), "lock helper exited with {status}");
        assert!(
            elapsed >= Duration::from_millis(300),
            "expected parent writer to wait for child-held lock, only waited {elapsed:?}"
        );

        let config = store(dir.path()).load().unwrap();
        assert!(config.profiles_for(Tool::Claude).contains_key("child"));
        assert!(config.profiles_for(Tool::Claude).contains_key("parent"));

        let contents = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        serde_json::from_str::<serde_json::Value>(&contents).unwrap();
    }

    #[test]
    fn config_mutations_fail_with_clear_error_after_lock_timeout() {
        let dir = tempdir().unwrap();
        let signal_path = dir.path().join("lock-timeout.signal");
        let mut child = spawn_lock_helper(dir.path(), &signal_path, 500, "child");

        wait_for_file(&signal_path);
        let err = store(dir.path())
            .with_mutating_config_timeout_for_test(Duration::from_millis(100), |config| {
                tool_profiles_mut(config, Tool::Claude)
                    .insert("parent".to_owned(), meta(AuthMethod::ApiKey));
                Ok(())
            })
            .unwrap_err();

        let status = child.wait().unwrap();
        assert!(status.success(), "lock helper exited with {status}");
        assert!(err
            .to_string()
            .contains("timed out waiting for config lock"));
        assert!(err
            .to_string()
            .contains("Another aisw command is updating configuration"));
    }

    #[test]
    fn lock_helper_process() {
        let Some(home) = std::env::var_os("AISW_CONFIG_LOCK_HELPER_HOME") else {
            return;
        };

        let signal_path =
            PathBuf::from(std::env::var_os("AISW_CONFIG_LOCK_HELPER_SIGNAL").unwrap());
        let hold_ms = std::env::var("AISW_CONFIG_LOCK_HELPER_HOLD_MS")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let profile_name = std::env::var("AISW_CONFIG_LOCK_HELPER_PROFILE").unwrap();

        let store = store(Path::new(&home));
        let _lock = store.acquire_lock().unwrap();
        fs::write(&signal_path, b"locked").unwrap();
        thread::sleep(Duration::from_millis(hold_ms));

        let mut config = store.load_unlocked().unwrap();
        tool_profiles_mut(&mut config, Tool::Claude).insert(profile_name, meta(AuthMethod::OAuth));
        store.save_unlocked(&config).unwrap();
    }

    fn spawn_lock_helper(
        home: &Path,
        signal_path: &Path,
        hold_ms: u64,
        profile_name: &str,
    ) -> std::process::Child {
        Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("config::tests::lock_helper_process")
            .arg("--nocapture")
            .env("AISW_CONFIG_LOCK_HELPER_HOME", home)
            .env("AISW_CONFIG_LOCK_HELPER_SIGNAL", signal_path)
            .env("AISW_CONFIG_LOCK_HELPER_HOLD_MS", hold_ms.to_string())
            .env("AISW_CONFIG_LOCK_HELPER_PROFILE", profile_name)
            .spawn()
            .unwrap()
    }

    fn wait_for_file(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }

        panic!("timed out waiting for {}", path.display());
    }
}
