use std::ffi::OsString;
use std::path::Path;

use anyhow::Result;

use crate::cli::StatusArgs;
use crate::config::{AuthMethod, ConfigStore};
use crate::profile::ProfileStore;
use crate::tool_detection;
use crate::types::Tool;

pub(crate) struct ToolStatus {
    pub tool: Tool,
    pub binary_found: bool,
    pub stored_profiles: usize,
    pub active_profile: Option<String>,
    pub auth_method: Option<String>,
    pub credentials_present: bool,
    pub permissions_ok: bool,
}

pub fn run(args: StatusArgs, home: &Path) -> Result<()> {
    run_in(args, home, std::env::var_os("PATH").unwrap_or_default())
}

pub(crate) fn run_in(args: StatusArgs, home: &Path, tool_path: OsString) -> Result<()> {
    let statuses = collect_status(home, &tool_path)?;
    if args.json {
        print_json(&statuses)?;
    } else {
        print_text(&statuses);
    }
    Ok(())
}

pub(crate) fn collect_status(home: &Path, tool_path: &OsString) -> Result<Vec<ToolStatus>> {
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;
    let profile_store = ProfileStore::new(home);

    let mut statuses = Vec::new();
    for tool in [Tool::Claude, Tool::Codex, Tool::Gemini] {
        let binary_found = tool_detection::detect_in(tool, tool_path.clone()).is_some();

        let active_name = match tool {
            Tool::Claude => config.active.claude.as_deref(),
            Tool::Codex => config.active.codex.as_deref(),
            Tool::Gemini => config.active.gemini.as_deref(),
        };
        let stored_profiles = match tool {
            Tool::Claude => config.profiles.claude.len(),
            Tool::Codex => config.profiles.codex.len(),
            Tool::Gemini => config.profiles.gemini.len(),
        };

        let (active_profile, auth_method, credentials_present, permissions_ok) =
            if let Some(name) = active_name {
                let profiles = match tool {
                    Tool::Claude => &config.profiles.claude,
                    Tool::Codex => &config.profiles.codex,
                    Tool::Gemini => &config.profiles.gemini,
                };
                let auth = profiles
                    .get(name)
                    .map(|m| auth_label(m.auth_method).to_owned());
                let profile_dir = profile_store.profile_dir(tool, name);
                let (creds, perms) = check_profile_dir(&profile_dir);
                (Some(name.to_owned()), auth, creds, perms)
            } else {
                (None, None, false, true)
            };

        statuses.push(ToolStatus {
            tool,
            binary_found,
            stored_profiles,
            active_profile,
            auth_method,
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
    "credentials present (validity not checked)"
}

fn print_text(statuses: &[ToolStatus]) {
    for s in statuses {
        let profile_info = match (&s.active_profile, &s.auth_method) {
            (Some(name), Some(auth)) => format!("{} ({})", name, auth),
            _ => "\u{2014}".to_owned(),
        };
        println!(
            "{:<16}  {:<24}  {}",
            s.tool.display_name(),
            profile_info,
            status_message(s)
        );
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
        let statuses = collect_status(tmp.path(), &empty_path()).unwrap();
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

        let statuses = collect_status(tmp.path(), &bin_dir.as_os_str().to_owned()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert!(claude.binary_found);
    }

    #[test]
    fn active_profile_reflected_in_status() {
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

        let statuses = collect_status(tmp.path(), &empty_path()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert_eq!(claude.active_profile.as_deref(), Some("work"));
        assert!(claude.credentials_present);
        assert!(claude.permissions_ok);
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

        let statuses = collect_status(tmp.path(), &empty_path()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert!(claude.credentials_present);
        assert!(!claude.permissions_ok);
    }

    #[test]
    fn run_in_exits_ok_with_no_config() {
        let tmp = tempdir().unwrap();
        run_in(status_args(false), tmp.path(), empty_path()).unwrap();
    }
}
