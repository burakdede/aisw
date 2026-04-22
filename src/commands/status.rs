use std::ffi::OsString;
use std::path::Path;

use anyhow::Result;

use crate::auth;
use crate::cli::StatusArgs;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend};
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
    pub credential_backend: Option<String>,
    pub state_mode: Option<String>,
    pub active_profile_added_at: Option<chrono::DateTime<chrono::Utc>>,
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
    let mut statuses = collect_status(home, user_home, &tool_path)?;
    apply_status_filters(&mut statuses, &args);
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
    credential_backend: crate::config::CredentialBackend,
    profile_store: &ProfileStore,
    profile_name: &str,
    user_home: &Path,
) -> Result<LiveActivation> {
    let applied = match tool {
        Tool::Claude => auth::claude::live_credentials_match(
            profile_store,
            profile_name,
            credential_backend,
            user_home,
        )?,
        Tool::Codex => auth::codex::live_files_match(
            profile_store,
            profile_name,
            credential_backend,
            user_home,
        )?,
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

fn should_skip_live_verification(tool: Tool, credential_backend: CredentialBackend) -> bool {
    cfg!(target_os = "macos")
        && tool == Tool::Claude
        && credential_backend == CredentialBackend::File
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
            credential_backend,
            active_profile_added_at,
            active_profile_applied,
            credentials_present,
            permissions_ok,
        ) = if let Some(name) = active_name {
            let profiles = config.profiles_for(tool);
            let profile_meta = &profiles[name];
            let profile_dir = profile_store.profile_dir(tool, name);
            let (creds, perms) =
                check_profile_storage(&profile_dir, tool, name, profile_meta.credential_backend);

            // --- Sync logic start ---
            maybe_sync_active_claude_profile(
                &config,
                &profile_store,
                tool,
                name,
                profile_meta.credential_backend,
                user_home,
            )?;
            // --- Sync logic end ---

            let auth = profiles
                .get(name)
                .map(|m| auth_label(m.auth_method).to_owned());
            let backend = profiles
                .get(name)
                .map(|m| m.credential_backend.display_name().to_owned());
            let added_at = profiles.get(name).map(|m| m.added_at);
            let applied = if creds {
                profiles
                    .get(name)
                    .map(|m| {
                        if should_skip_live_verification(tool, m.credential_backend) {
                            Ok(None)
                        } else {
                            assess_live_state(
                                tool,
                                m.auth_method,
                                m.credential_backend,
                                &profile_store,
                                name,
                                user_home,
                            )
                            .map(|state| {
                                Some(match state {
                                    LiveActivation::Applied => true,
                                    LiveActivation::NotApplied => false,
                                })
                            })
                        }
                    })
                    .transpose()?
                    .flatten()
            } else {
                Some(false)
            };
            (
                Some(name.to_owned()),
                auth,
                backend,
                added_at,
                applied,
                creds,
                perms,
            )
        } else {
            (None, None, None, None, None, false, true)
        };

        statuses.push(ToolStatus {
            tool,
            binary_found,
            stored_profiles,
            active_profile,
            auth_method,
            credential_backend,
            state_mode,
            active_profile_added_at,
            active_profile_applied,
            credentials_present,
            permissions_ok,
        });
    }
    Ok(statuses)
}

fn maybe_sync_active_claude_profile(
    config: &crate::config::Config,
    profile_store: &ProfileStore,
    tool: Tool,
    profile_name: &str,
    backend: CredentialBackend,
    user_home: &Path,
) -> Result<()> {
    if tool != Tool::Claude
        || config.state_mode_for(Tool::Claude) != crate::types::StateMode::Shared
    {
        return Ok(());
    }

    let _ = auth::claude::sync_profile_from_live_if_same_identity(
        profile_store,
        profile_name,
        backend,
        user_home,
    )?;
    Ok(())
}

fn apply_status_filters(statuses: &mut Vec<ToolStatus>, args: &StatusArgs) {
    if let Some(tool) = args.tool {
        statuses.retain(|s| s.tool == tool);
    }

    if args.active_only {
        statuses.retain(|s| s.active_profile.is_some());
    }

    if let Some(search) = args.search.as_deref() {
        let needle = search.trim().to_ascii_lowercase();
        if !needle.is_empty() {
            statuses.retain(|s| {
                s.tool.binary_name().to_ascii_lowercase().contains(&needle)
                    || s.tool.display_name().to_ascii_lowercase().contains(&needle)
                    || s.active_profile
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(&needle)
                    || s.auth_method
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(&needle)
                    || s.credential_backend
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(&needle)
            });
        }
    }

    match args.sort {
        Some(crate::cli::SortBy::Name) => {
            statuses.sort_by_key(|s| s.tool.binary_name());
        }
        Some(crate::cli::SortBy::Recent) => {
            statuses.sort_by(|a, b| {
                b.active_profile_added_at
                    .cmp(&a.active_profile_added_at)
                    .then_with(|| a.tool.binary_name().cmp(b.tool.binary_name()))
            });
        }
        None => {}
    }
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
fn check_profile_storage(
    dir: &Path,
    tool: Tool,
    profile_name: &str,
    credential_backend: CredentialBackend,
) -> (bool, bool) {
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
    let credentials_present = match credential_backend {
        CredentialBackend::File => found_file,
        CredentialBackend::SystemKeyring => {
            auth::secure_store::read_profile_secret(tool, profile_name)
                .ok()
                .flatten()
                .is_some()
        }
    };

    (credentials_present, perms_ok)
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
        return match s.credential_backend.as_deref() {
            Some("system_keyring") => "secure credentials missing from the managed system keyring",
            _ => "credential files missing",
        };
    }
    if !s.permissions_ok {
        return "credentials present \u{2014} permissions too broad!";
    }
    if s.tool == Tool::Claude
        && s.credential_backend.as_deref() == Some("file")
        && cfg!(target_os = "macos")
        && s.active_profile_applied.is_none()
    {
        return "credentials present (live macOS Keychain not checked)";
    }
    if s.active_profile_applied == Some(false) {
        return "credentials present, but live tool config does not match the active profile";
    }
    "credentials present (validity not checked)"
}

fn backend_diagnostic(s: &ToolStatus) -> Option<String> {
    if s.credential_backend.as_deref() == Some("system_keyring") {
        return auth::system_keyring::usability_diagnostic();
    }
    None
}

fn print_text(statuses: &[ToolStatus]) {
    const ACTIVE_WIDTH: usize = 36;
    const AUTH_WIDTH: usize = 18;
    const BACKEND_WIDTH: usize = 28;
    const STATE_MODE_WIDTH: usize = 14;
    const STATE_WIDTH: usize = 80;

    output::print_title("Status");

    for s in statuses {
        output::print_tool_section(s.tool);
        output::print_kv(
            "Active",
            output::ellipsize(
                output::active_value(s.active_profile.as_deref()),
                ACTIVE_WIDTH,
            ),
        );
        if let Some(auth) = s.auth_method.as_deref() {
            output::print_kv("Auth", output::ellipsize(auth, AUTH_WIDTH));
        }
        if let Some(backend) = s.credential_backend.as_deref() {
            output::print_kv("Backend", output::ellipsize(backend, BACKEND_WIDTH));
        }
        if let Some(mode) = s.state_mode.as_deref() {
            output::print_kv("State mode", output::ellipsize(mode, STATE_MODE_WIDTH));
        }
        output::print_kv("State", output::ellipsize(status_message(s), STATE_WIDTH));
        if let Some(diagnostic) = backend_diagnostic(s) {
            output::print_warning(diagnostic);
        }
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
                "credential_backend":   s.credential_backend,
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

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

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

    fn make_fake_security_binary(dir: &Path) -> PathBuf {
        let path = dir.join("security");
        fs::write(
            &path,
            "#!/bin/sh\n\
             store_root=\"${AISW_KEYRING_TEST_DIR:-$HOME/keychain}\"\n\
             item_dir() {\n\
               printf '%s/%s/%s' \"$store_root\" \"$1\" \"$2\"\n\
             }\n\
             cmd=\"$1\"\n\
             shift\n\
             service=''\n\
             account=''\n\
             password=''\n\
             want_secret='false'\n\
             while [ \"$#\" -gt 0 ]; do\n\
               case \"$1\" in\n\
                 -s)\n\
                   shift\n\
                   service=\"$1\"\n\
                   ;;\n\
                 -a)\n\
                   shift\n\
                   account=\"$1\"\n\
                   ;;\n\
                 -w)\n\
                   if [ \"$cmd\" = \"find-generic-password\" ]; then\n\
                     want_secret='true'\n\
                   else\n\
                     shift\n\
                     password=\"$1\"\n\
                   fi\n\
                   ;;\n\
               esac\n\
               shift\n\
             done\n\
             item=\"$(item_dir \"$service\" \"$account\")\"\n\
             case \"$cmd\" in\n\
               find-generic-password)\n\
                 if [ ! -f \"$item/secret\" ]; then\n\
                   echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
                   exit 44\n\
                 fi\n\
                 if [ \"$want_secret\" = 'true' ]; then\n\
                   /bin/cat \"$item/secret\"\n\
                 else\n\
                   acct=$(/bin/cat \"$item/account\")\n\
                   printf 'keychain: \"/fake/login.keychain-db\"\\n'\n\
                   printf 'attributes:\\n'\n\
                   printf '    \"acct\"<blob>=\"%s\"\\n' \"$acct\"\n\
                 fi\n\
                 ;;\n\
               add-generic-password)\n\
                 /bin/mkdir -p \"$item\"\n\
                 printf '%s' \"$account\" > \"$item/account\"\n\
                 printf '%s' \"$password\" > \"$item/secret\"\n\
                 ;;\n\
               delete-generic-password)\n\
                 if [ -d \"$item\" ]; then\n\
                   /bin/rm -rf \"$item\"\n\
                 else\n\
                   echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
                   exit 44\n\
                 fi\n\
                 ;;\n\
               *)\n\
                 echo \"unexpected security command: $cmd\" >&2\n\
                 exit 1\n\
                 ;;\n\
             esac\n",
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn status_args(json: bool) -> StatusArgs {
        StatusArgs {
            tool: None,
            search: None,
            sort: None,
            active_only: false,
            json,
        }
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

        auth::claude::apply_live_credentials(&ps, "work", CredentialBackend::File, tmp.path())
            .unwrap();

        let statuses = collect_status(tmp.path(), tmp.path(), &empty_path()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert_eq!(claude.active_profile.as_deref(), Some("work"));
        if cfg!(target_os = "macos") {
            assert_eq!(claude.active_profile_applied, None);
        } else {
            assert_eq!(claude.active_profile_applied, Some(true));
        }
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
            CredentialBackend::File,
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
        auth::claude::apply_live_credentials(
            &profile_store,
            "work",
            CredentialBackend::File,
            tmp.path(),
        )
        .unwrap();

        let status = assess_live_state(
            Tool::Claude,
            AuthMethod::ApiKey,
            CredentialBackend::File,
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
            CredentialBackend::File,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn macos_claude_file_profile_skips_live_keychain_verification() {
        if !cfg!(target_os = "macos") {
            return;
        }

        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&profile_store, &config_store, "work", CLAUDE_KEY, None).unwrap();
        config_store.set_active(Tool::Claude, "work").unwrap();

        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let statuses =
            collect_status(tmp.path(), tmp.path(), &bin_dir.as_os_str().to_owned()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert_eq!(claude.credential_backend.as_deref(), Some("file"));
        assert_eq!(claude.active_profile_applied, None);
        assert_eq!(
            status_message(claude),
            "credentials present (live macOS Keychain not checked)"
        );
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
            CredentialBackend::File,
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
            CredentialBackend::File,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn codex_system_keyring_live_state_is_applied_when_live_files_match() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let security_bin = make_fake_security_binary(&bin_dir);
        let keyring_dir = tmp.path().join("keychain");
        let _security_bin_guard =
            EnvVarGuard::set("AISW_SECURITY_BIN", &security_bin.display().to_string());
        let _keyring_dir_guard =
            EnvVarGuard::set("AISW_KEYRING_TEST_DIR", &keyring_dir.display().to_string());
        let _user_guard = EnvVarGuard::set("USER", "tester");

        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::codex::add_api_key(&profile_store, &config_store, "work", CODEX_KEY, None).unwrap();
        let auth_bytes = profile_store
            .read_file(Tool::Codex, "work", "auth.json")
            .unwrap();
        auth::secure_store::write_profile_secret(Tool::Codex, "work", &auth_bytes).unwrap();
        std::fs::remove_file(
            profile_store
                .profile_dir(Tool::Codex, "work")
                .join("auth.json"),
        )
        .unwrap();
        auth::codex::apply_live_credentials(
            &profile_store,
            "work",
            CredentialBackend::SystemKeyring,
            tmp.path(),
        )
        .unwrap();

        let status = assess_live_state(
            Tool::Codex,
            AuthMethod::ApiKey,
            CredentialBackend::SystemKeyring,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::Applied);
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
            CredentialBackend::File,
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
            CredentialBackend::File,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn apply_status_filters_honors_tool_search_and_active_only() {
        let mut statuses = vec![
            ToolStatus {
                tool: Tool::Claude,
                binary_found: true,
                stored_profiles: 1,
                active_profile: Some("work".to_owned()),
                auth_method: Some("api_key".to_owned()),
                credential_backend: Some("file".to_owned()),
                state_mode: Some("isolated".to_owned()),
                active_profile_added_at: Some(chrono::Utc::now()),
                active_profile_applied: Some(true),
                credentials_present: true,
                permissions_ok: true,
            },
            ToolStatus {
                tool: Tool::Codex,
                binary_found: true,
                stored_profiles: 1,
                active_profile: None,
                auth_method: None,
                credential_backend: None,
                state_mode: Some("isolated".to_owned()),
                active_profile_added_at: None,
                active_profile_applied: None,
                credentials_present: false,
                permissions_ok: true,
            },
        ];

        let args = StatusArgs {
            tool: Some(Tool::Claude),
            search: Some("work".to_owned()),
            sort: None,
            active_only: true,
            json: false,
        };
        apply_status_filters(&mut statuses, &args);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].tool, Tool::Claude);
    }

    #[test]
    fn apply_status_filters_sort_recent_orders_newest_first() {
        let older = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let newer = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let mut statuses = vec![
            ToolStatus {
                tool: Tool::Claude,
                binary_found: true,
                stored_profiles: 1,
                active_profile: Some("old".to_owned()),
                auth_method: Some("api_key".to_owned()),
                credential_backend: Some("file".to_owned()),
                state_mode: Some("isolated".to_owned()),
                active_profile_added_at: Some(older),
                active_profile_applied: Some(true),
                credentials_present: true,
                permissions_ok: true,
            },
            ToolStatus {
                tool: Tool::Codex,
                binary_found: true,
                stored_profiles: 1,
                active_profile: Some("new".to_owned()),
                auth_method: Some("api_key".to_owned()),
                credential_backend: Some("file".to_owned()),
                state_mode: Some("isolated".to_owned()),
                active_profile_added_at: Some(newer),
                active_profile_applied: Some(true),
                credentials_present: true,
                permissions_ok: true,
            },
        ];
        let args = StatusArgs {
            tool: None,
            search: None,
            sort: Some(crate::cli::SortBy::Recent),
            active_only: false,
            json: false,
        };

        apply_status_filters(&mut statuses, &args);
        assert_eq!(statuses[0].tool, Tool::Codex);
        assert_eq!(statuses[1].tool, Tool::Claude);
    }

    #[test]
    fn shared_claude_oauth_status_refreshes_active_profile_from_live() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(user_home.join(".claude")).unwrap();

        let ps = ProfileStore::new(&home);
        let cs = ConfigStore::new(&home);

        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            ".credentials.json",
            br#"{"claudeAiOauth":{"accessToken":"old","refreshToken":"old-refresh","expiresAt":1000}}"#,
        )
        .unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            "oauth-account.json",
            br#"{"emailAddress":"work@example.com","organizationUuid":"org-123"}"#,
        )
        .unwrap();
        cs.add_profile(
            Tool::Claude,
            "work",
            crate::config::ProfileMeta {
                added_at: chrono::Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: crate::config::CredentialBackend::File,
                label: None,
            },
        )
        .unwrap();
        cs.set_active(Tool::Claude, "work").unwrap();
        cs.set_state_mode(Tool::Claude, crate::types::StateMode::Shared)
            .unwrap();

        // Prepare live state with NEWER token but SAME identity
        fs::write(
            user_home.join(".claude").join(".credentials.json"),
            br#"{"claudeAiOauth":{"accessToken":"new","refreshToken":"new-refresh","expiresAt":2000}}"#,
        )
        .unwrap();
        fs::write(
            user_home.join(".claude.json"),
            br#"{"oauthAccount":{"emailAddress":"work@example.com","organizationUuid":"org-123"}}"#,
        )
        .unwrap();

        // Collect status - this should trigger the refresh
        let statuses = collect_status(&home, &user_home, &OsString::new()).unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert_eq!(claude.active_profile.as_deref(), Some("work"));

        // Verify stored state is updated
        let stored = ps
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        let refreshed_live = br#"{"claudeAiOauth":{"accessToken":"new","refreshToken":"new-refresh","expiresAt":2000}}"#;
        assert_eq!(stored, refreshed_live);
    }
}
