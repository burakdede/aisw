use std::ffi::OsString;
use std::path::Path;

use anyhow::Result;

use crate::auth;
use crate::cli::StatusArgs;
use crate::config::{AuthMethod, ConfigStore};
use crate::output;
use crate::profile::ProfileStore;
use crate::tool_detection;
use crate::types::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveActivation {
    Applied,
    NotApplied,
}

pub(crate) struct ToolStatus {
    pub tool: Tool,
    pub binary_found: bool,
    pub stored_profiles: usize,
    pub active_profile: Option<String>,
    pub auth_method: Option<String>,
    pub state_mode: Option<String>,
    pub active_profile_applied: Option<bool>,
    pub credentials_present: bool,
    pub permissions_ok: bool,
}

pub fn run(args: StatusArgs, home: &Path) -> Result<()> {
    let user_home = dirs::home_dir().unwrap_or_else(|| Path::new(".").to_path_buf());
    run_in(
        args,
        home,
        &user_home,
        std::env::var_os("PATH").unwrap_or_default(),
    )
}

pub(crate) fn run_in(
    args: StatusArgs,
    home: &Path,
    user_home: &Path,
    tool_path: OsString,
) -> Result<()> {
    let statuses = collect_status(home, user_home, &tool_path)?;
    if args.json {
        print_json(&statuses)?;
    } else {
        print_text(&statuses);
    }
    Ok(())
}

fn assess_live_state(
    tool: Tool,
    auth_method: AuthMethod,
    profile_store: &ProfileStore,
    profile_name: &str,
    user_home: &Path,
) -> Result<LiveActivation> {
    let applied = match tool {
        Tool::Claude => {
            auth::claude::live_credentials_match(profile_store, profile_name, user_home)?
        }
        Tool::Codex => auth::codex::live_files_match(profile_store, profile_name, user_home)?,
        Tool::Gemini => match auth_method {
            AuthMethod::ApiKey => auth::gemini::live_env_matches(
                profile_store,
                profile_name,
                &user_home.join(".gemini").join(".env"),
            )?,
            AuthMethod::OAuth => auth::gemini::live_token_cache_matches(
                profile_store,
                profile_name,
                &user_home.join(".gemini"),
            )?,
        },
    };

    if applied {
        Ok(LiveActivation::Applied)
    } else {
        Ok(LiveActivation::NotApplied)
    }
}

pub(crate) fn collect_status(
    home: &Path,
    user_home: &Path,
    tool_path: &OsString,
) -> Result<Vec<ToolStatus>> {
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;
    let profile_store = ProfileStore::new(home);

    let mut statuses = Vec::new();
    for tool in Tool::ALL {
        let binary_found = tool_detection::detect_in(tool, tool_path.clone()).is_some();

        let active_name = config.active_for(tool);
        let stored_profiles = config.profiles_for(tool).len();

        let state_mode = if tool.supports_state_mode() {
            Some(config.state_mode_for(tool).display_name().to_owned())
        } else {
            None
        };

        let (
            active_profile,
            auth_method,
            active_profile_applied,
            credentials_present,
            permissions_ok,
        ) = if let Some(name) = active_name {
            let profiles = config.profiles_for(tool);
            let auth = profiles
                .get(name)
                .map(|m| auth_label(m.auth_method).to_owned());
            let applied = profiles
                .get(name)
                .map(|m| assess_live_state(tool, m.auth_method, &profile_store, name, user_home))
                .transpose()?
                .map(|state| match state {
                    LiveActivation::Applied => true,
                    LiveActivation::NotApplied => false,
                });
            let profile_dir = profile_store.profile_dir(tool, name);
            let (creds, perms) = check_profile_dir(&profile_dir);
            (Some(name.to_owned()), auth, applied, creds, perms)
        } else {
            (None, None, None, false, true)
        };

        statuses.push(ToolStatus {
            tool,
            binary_found,
            stored_profiles,
            active_profile,
            auth_method,
            state_mode,
            active_profile_applied,
            credentials_present,
            permissions_ok,
        });
    }
    Ok(statuses)
}

fn auth_label(method: AuthMethod) -> &'static str {
    match method {
        AuthMethod::OAuth => "oauth",
        AuthMethod::ApiKey => "api_key",
    }
}

/// Returns (credentials_present, permissions_ok).
/// credentials_present: at least one regular file exists in the dir.
/// permissions_ok: all regular files have 0600 permissions (unix only).
fn check_profile_dir(dir: &Path) -> (bool, bool) {
    if !dir.is_dir() {
        return (false, true);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (false, true);
    };
    let mut found_file = false;
    let mut perms_ok = true;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_symlink() || !path.is_file() {
            continue;
        }
        found_file = true;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.permissions().mode() & 0o777 != 0o600 {
                    perms_ok = false;
                }
            }
        }
    }
    (found_file, perms_ok)
}

fn status_message(s: &ToolStatus) -> &'static str {
    if !s.binary_found {
        return "binary not found";
    }
    if s.active_profile.is_none() {
        if s.stored_profiles > 0 {
            return "profiles stored, but none is active";
        }
        return "no active profile";
    }
    if !s.credentials_present {
        return "credential files missing";
    }
    if !s.permissions_ok {
        return "credentials present \u{2014} permissions too broad!";
    }
    if s.active_profile_applied == Some(false) {
        return "credentials present, but live tool config does not match the active profile";
    }
    "credentials present (validity not checked)"
}

fn print_text(statuses: &[ToolStatus]) {
    output::print_title("Status");

    for s in statuses {
        output::print_tool_section(s.tool);
        output::print_kv("Active", output::active_value(s.active_profile.as_deref()));
        if let Some(auth) = s.auth_method.as_deref() {
            output::print_kv("Auth", auth);
        }
        if let Some(mode) = s.state_mode.as_deref() {
            output::print_kv("State mode", mode);
        }
        output::print_kv("State", status_message(s));
        output::print_blank_line();
    }
}

fn print_json(statuses: &[ToolStatus]) -> Result<()> {
    let json: Vec<serde_json::Value> = statuses
        .iter()
        .map(|s| {
            serde_json::json!({
                "tool":                 s.tool.binary_name(),
                "binary_found":         s.binary_found,
                "stored_profiles":      s.stored_profiles,
                "active_profile":       s.active_profile,
                "auth_method":          s.auth_method,
                "state_mode":           s.state_mode,
                "active_profile_applied": s.active_profile_applied,
                "credentials_present":  s.credentials_present,
                "permissions_ok":       s.permissions_ok,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::cli::StatusArgs;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    const CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const CODEX_KEY: &str = "sk-codex-test-key-12345";
    const GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    fn empty_path() -> OsString {
        OsString::from("")
    }

    fn make_fake_binary(dir: &Path, name: &str) {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\necho 'fake 1.0'\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn status_args(json: bool) -> StatusArgs {
        StatusArgs { json }
    }

    #[test]
    fn empty_config_no_path_all_not_found() {
        let tmp = tempdir().unwrap();
        let statuses = collect_status(tmp.path(), tmp.path(), &empty_path()).unwrap();
        assert_eq!(statuses.len(), 3);
        assert!(statuses.iter().all(|s| !s.binary_found));
        assert!(statuses.iter().all(|s| s.active_profile.is_none()));
    }

    #[test]
    fn binary_found_when_in_path() {
        let tmp = tempdir().unwrap();
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let statuses =
            collect_status(tmp.path(), tmp.path(), &bin_dir.as_os_str().to_owned()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert!(claude.binary_found);
    }

    #[test]
    fn active_profile_reflected_in_status() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(
            &ps,
            &cs,
            "work",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            None,
        )
        .unwrap();
        cs.set_active(Tool::Claude, "work").unwrap();

        auth::claude::apply_live_credentials(&ps, "work", tmp.path()).unwrap();

        let statuses = collect_status(tmp.path(), tmp.path(), &empty_path()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert_eq!(claude.active_profile.as_deref(), Some("work"));
        assert_eq!(claude.active_profile_applied, Some(true));
        assert!(claude.credentials_present);
        assert!(claude.permissions_ok);
    }

    #[test]
    fn gemini_active_profile_applied_reflects_live_state() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::gemini::add_api_key(&ps, &cs, "work", "AIzatest1234567890ABCDEF", None).unwrap();
        cs.set_active(Tool::Gemini, "work").unwrap();
        std::fs::create_dir_all(tmp.path().join(".gemini")).unwrap();
        auth::gemini::apply_env_file(&ps, "work", &tmp.path().join(".gemini").join(".env"))
            .unwrap();

        let statuses = collect_status(tmp.path(), tmp.path(), &empty_path()).unwrap();
        let gemini = statuses.iter().find(|s| s.tool == Tool::Gemini).unwrap();
        assert_eq!(gemini.active_profile_applied, Some(true));
    }

    #[test]
    fn broad_permissions_detected() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(
            &ps,
            &cs,
            "work",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            None,
        )
        .unwrap();
        cs.set_active(Tool::Claude, "work").unwrap();

        // Widen permissions on the credential file.
        let cred = ps
            .profile_dir(Tool::Claude, "work")
            .join(".credentials.json");
        fs::set_permissions(&cred, fs::Permissions::from_mode(0o644)).unwrap();

        let statuses = collect_status(tmp.path(), tmp.path(), &empty_path()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert!(claude.credentials_present);
        assert!(!claude.permissions_ok);
    }

    #[test]
    fn run_in_exits_ok_with_no_config() {
        let tmp = tempdir().unwrap();
        run_in(status_args(false), tmp.path(), tmp.path(), empty_path()).unwrap();
    }

    #[test]
    fn gemini_api_key_live_state_is_not_applied_without_live_env() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::gemini::add_api_key(&profile_store, &config_store, "work", GEMINI_KEY, None).unwrap();

        let status = assess_live_state(
            Tool::Gemini,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn claude_live_state_is_applied_when_live_credentials_match() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&profile_store, &config_store, "work", CLAUDE_KEY, None).unwrap();
        auth::claude::apply_live_credentials(&profile_store, "work", tmp.path()).unwrap();

        let status = assess_live_state(
            Tool::Claude,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::Applied);
    }

    #[test]
    fn claude_live_state_is_not_applied_when_live_credentials_are_missing() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&profile_store, &config_store, "work", CLAUDE_KEY, None).unwrap();

        let status = assess_live_state(
            Tool::Claude,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn codex_live_state_is_applied_when_live_files_match() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::codex::add_api_key(&profile_store, &config_store, "work", CODEX_KEY, None).unwrap();
        auth::codex::apply_live_files(&profile_store, "work", tmp.path()).unwrap();

        let status = assess_live_state(
            Tool::Codex,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::Applied);
    }

    #[test]
    fn codex_live_state_is_not_applied_when_live_files_are_missing() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::codex::add_api_key(&profile_store, &config_store, "work", CODEX_KEY, None).unwrap();

        let status = assess_live_state(
            Tool::Codex,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn gemini_oauth_live_state_is_applied_when_cache_matches() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        profile_store.create(Tool::Gemini, "work").unwrap();
        profile_store
            .write_file(
                Tool::Gemini,
                "work",
                "oauth_creds.json",
                br#"{"token":"tok"}"#,
            )
            .unwrap();
        profile_store
            .write_file(
                Tool::Gemini,
                "work",
                "settings.json",
                br#"{"account":"work"}"#,
            )
            .unwrap();
        auth::gemini::apply_token_cache(&profile_store, "work", &tmp.path().join(".gemini"))
            .unwrap();

        let status = assess_live_state(
            Tool::Gemini,
            AuthMethod::OAuth,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::Applied);
    }

    #[test]
    fn gemini_oauth_live_state_is_not_applied_when_cache_is_missing() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        profile_store.create(Tool::Gemini, "work").unwrap();
        profile_store
            .write_file(
                Tool::Gemini,
                "work",
                "oauth_creds.json",
                br#"{"token":"tok"}"#,
            )
            .unwrap();

        let status = assess_live_state(
            Tool::Gemini,
            AuthMethod::OAuth,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }
}
