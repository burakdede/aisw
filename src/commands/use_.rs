use std::path::Path;

use anyhow::{Context, Result};

use crate::auth;
use crate::backup::BackupManager;
use crate::cli::UseArgs;
use crate::config::{AuthMethod, ConfigStore};
use crate::error::AiswError;
use crate::output;
use crate::profile::ProfileStore;
use crate::types::{StateMode, Tool};

pub fn run(args: UseArgs, home: &Path) -> Result<()> {
    let user_home = dirs::home_dir().context("could not determine home directory")?;
    if args.all {
        let profile_name = args.all_profile.as_deref().unwrap_or_default();
        if profile_name.is_empty() {
            anyhow::bail!("--all requires --profile <name>");
        }
        run_all_in(profile_name, home, &user_home)
    } else {
        let tool = args.tool.expect("tool required when --all is not set");
        let profile_name = args
            .profile_name
            .expect("profile_name required when tool is set");
        run_for_tool(
            tool,
            &profile_name,
            args.state_mode,
            args.emit_env,
            home,
            &user_home,
        )
    }
}

pub(crate) fn run_all_in(profile_name: &str, home: &Path, user_home: &Path) -> Result<()> {
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;
    let mut switched = 0usize;
    let mut errors = Vec::new();

    for tool in Tool::ALL {
        let profiles = config.profiles_for(tool);
        if !profiles.contains_key(profile_name) {
            output::print_info(format!(
                "(skipped {} — no profile named '{}')",
                tool, profile_name
            ));
            continue;
        }
        match run_for_tool(tool, profile_name, None, false, home, user_home) {
            Ok(()) => switched += 1,
            Err(e) => errors.push(format!("{}: {}", tool, e)),
        }
    }

    if switched == 0 && errors.is_empty() {
        anyhow::bail!("no tool has a profile named '{}'", profile_name);
    }
    for e in &errors {
        output::print_warning(e);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn run_in(args: UseArgs, home: &Path, user_home: &Path) -> Result<()> {
    let tool = args.tool.expect("tool is required in run_in");
    let profile_name = args.profile_name.expect("profile_name required in run_in");
    run_for_tool(
        tool,
        &profile_name,
        args.state_mode,
        args.emit_env,
        home,
        user_home,
    )
}

fn run_for_tool(
    tool: Tool,
    profile_name: &str,
    state_mode_override: Option<StateMode>,
    emit_env: bool,
    home: &Path,
    user_home: &Path,
) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;
    let requested_state_mode = match (tool, state_mode_override) {
        (t, mode) if t.supports_state_mode() => mode,
        (_, Some(_)) => {
            anyhow::bail!(
                "--state-mode is currently supported only for claude and codex.\n  \
                 Gemini remains isolated-only because its native ~/.gemini directory mixes \
                 credentials with broader local state such as history, trusted folders, \
                 project mappings, settings, and MCP config."
            );
        }
        (_, None) => None,
    };
    let state_mode = if tool.supports_state_mode() {
        requested_state_mode.unwrap_or(config.state_mode_for(tool))
    } else {
        StateMode::Isolated
    };

    let profiles = config.profiles_for(tool);

    let profile_meta = match profiles.get(profile_name) {
        Some(m) => m,
        None => {
            let profile_names: Vec<&str> = profiles.keys().map(String::as_str).collect();
            let suggestion =
                crate::util::edit_distance::closest_match(profile_name, &profile_names, 2);
            let err = AiswError::ProfileNotFound {
                tool,
                name: profile_name.to_owned(),
            };
            if let Some(hint) = suggestion {
                anyhow::bail!("{}\n  Did you mean '{}'?", err, hint);
            } else {
                return Err(err.into());
            }
        }
    };
    profile_meta.credential_backend.validate_for_tool(tool)?;

    if config.settings.backup_on_switch {
        let backup_manager = BackupManager::new(home);
        let profile_dir = profile_store.profile_dir(tool, profile_name);
        backup_manager.snapshot(tool, profile_name, &profile_dir, profile_meta)?;
    }

    match tool {
        Tool::Claude => match profile_meta.auth_method {
            AuthMethod::OAuth => {
                if emit_env {
                    auth::claude::emit_shell_env(profile_name, &profile_store, state_mode);
                } else {
                    if cfg!(target_os = "macos") {
                        output::print_info(
                            "Claude on macOS stores live auth in Keychain. Switching this profile may trigger a macOS Keychain prompt so aisw can update Claude's active credentials.",
                        );
                        output::print_blank_line();
                    }
                    auth::claude::apply_live_credentials(
                        &profile_store,
                        profile_name,
                        profile_meta.credential_backend,
                        user_home,
                    )?;
                }
            }
            AuthMethod::ApiKey => {
                if emit_env {
                    auth::claude::emit_shell_env(profile_name, &profile_store, state_mode);
                } else {
                    if cfg!(target_os = "macos") {
                        output::print_info(
                            "Claude on macOS stores live auth in Keychain. Switching this profile may trigger a macOS Keychain prompt so aisw can update Claude's active credentials.",
                        );
                        output::print_blank_line();
                    }
                    auth::claude::apply_live_credentials(
                        &profile_store,
                        profile_name,
                        profile_meta.credential_backend,
                        user_home,
                    )?;
                }
            }
        },
        Tool::Codex => match profile_meta.auth_method {
            AuthMethod::OAuth => {
                if emit_env {
                    auth::codex::emit_shell_env(profile_name, &profile_store, state_mode);
                } else {
                    auth::codex::apply_live_credentials(
                        &profile_store,
                        profile_name,
                        profile_meta.credential_backend,
                        user_home,
                    )?;
                }
            }
            AuthMethod::ApiKey => {
                if emit_env {
                    match state_mode {
                        StateMode::Isolated => {
                            auth::codex::emit_shell_env(profile_name, &profile_store, state_mode)
                        }
                        StateMode::Shared => {
                            crate::auth::files::emit_unset("CODEX_HOME");
                        }
                    }
                } else {
                    auth::codex::apply_live_credentials(
                        &profile_store,
                        profile_name,
                        profile_meta.credential_backend,
                        user_home,
                    )?;
                }
            }
        },
        Tool::Gemini => {
            let gemini_dir = user_home.join(".gemini");
            std::fs::create_dir_all(&gemini_dir)
                .with_context(|| format!("could not create {}", gemini_dir.display()))?;
            match profile_meta.auth_method {
                AuthMethod::ApiKey => {
                    if emit_env {
                        let key = auth::gemini::read_api_key(&profile_store, profile_name)?;
                        crate::auth::files::emit_export("GEMINI_API_KEY", &key);
                    } else {
                        auth::gemini::apply_env_file(
                            &profile_store,
                            profile_name,
                            &gemini_dir.join(".env"),
                        )?;
                    }
                }
                AuthMethod::OAuth => {
                    if emit_env {
                        crate::auth::files::emit_unset("GEMINI_API_KEY");
                    } else {
                        auth::gemini::apply_token_cache(&profile_store, profile_name, &gemini_dir)?;
                    }
                }
            }
        }
    }

    config_store.activate_profile(
        tool,
        profile_name,
        tool.supports_state_mode().then_some(state_mode),
    )?;

    if !emit_env {
        let title = format!("{} \u{2192} {}", tool.display_name(), profile_name);
        output::print_title(&title);
        output::print_kv("Auth", auth_label(profile_meta.auth_method));
        output::print_kv("Backend", profile_meta.credential_backend.display_name());
        if let Some(identity) = extract_switch_identity(&profile_store, tool, profile_name) {
            output::print_kv("Account", &identity);
        }
        if tool.supports_state_mode() {
            output::print_kv("State mode", state_mode.display_name());
        }
        output::print_blank_line();
        output::print_effects_header();
        output::print_effect("Live tool configuration updated.");
        output::print_effect("Active profile updated.");
        if tool.supports_state_mode() {
            output::print_effect(match (tool, state_mode) {
                (Tool::Claude, StateMode::Isolated) => {
                    "Claude will use isolated profile state when shell integration is active."
                }
                (Tool::Claude, StateMode::Shared) => {
                    "Claude will keep shared local state and only switch account credentials."
                }
                (Tool::Codex, StateMode::Isolated) => {
                    "Codex will use isolated profile state when shell integration is active."
                }
                (Tool::Codex, StateMode::Shared) => {
                    "Codex will keep shared local state and only switch account credentials."
                }
                (Tool::Gemini, _) => unreachable!(),
            });
        }
        if config.settings.backup_on_switch {
            output::print_effect("Backup created before switching.");
        }
        output::print_blank_line();
        output::print_next_step(output::next_step_after_use());
    }

    Ok(())
}

fn auth_label(method: AuthMethod) -> &'static str {
    match method {
        AuthMethod::OAuth => "oauth",
        AuthMethod::ApiKey => "api-key",
    }
}

/// Best-effort: extract a human-readable account identity from stored credentials.
/// Returns `None` silently when no identity is parseable — never fails the switch.
fn extract_switch_identity(profile_store: &ProfileStore, tool: Tool, name: &str) -> Option<String> {
    let cred_file = match tool {
        Tool::Claude => ".credentials.json",
        Tool::Codex => "auth.json",
        Tool::Gemini => "oauth_creds.json",
    };

    let bytes = profile_store.read_file(tool, name, cred_file).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;

    // Try common email/identity fields in order of specificity.
    // Claude OAuth: {"oauthAccount":{"emailAddress":"..."}}, {"account":{"email":"..."}}
    // Codex OAuth:  {"account":{"email":"..."}}
    // Codex JWT:    {"token":"<jwt>"} — decode middle segment
    for path in &[
        &["oauthAccount", "emailAddress"] as &[&str],
        &["account", "email"],
        &["emailAddress"],
        &["email"],
        &["account", "emailAddress"],
    ] {
        if let Some(s) = json_path(&v, path) {
            return Some(s);
        }
    }

    // For Codex API-key profiles the "token" field may be a JWT.
    if tool == Tool::Codex {
        if let Some(jwt) = v.get("token").and_then(|t| t.as_str()) {
            if let Some(email) = decode_jwt_email(jwt) {
                return Some(email);
            }
        }
    }

    None
}

fn json_path(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str().map(|s| s.to_owned())
}

/// Decode the payload segment of a JWT and extract the `email` claim, if present.
fn decode_jwt_email(jwt: &str) -> Option<String> {
    let payload_b64 = jwt.split('.').nth(1)?;
    // JWT uses base64url without padding.
    let padded = base64_url_to_padded(payload_b64);
    let bytes = base64_decode(&padded)?;
    let payload: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    payload
        .get("email")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned())
}

fn base64_url_to_padded(s: &str) -> String {
    let mut out = s.replace('-', "+").replace('_', "/");
    match out.len() % 4 {
        2 => out.push_str("=="),
        3 => out.push('='),
        _ => {}
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    // Minimal inline base64 decoder for JWT payloads.
    // We only need this for JWT payloads so a simple table-based decode is fine.
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut table = [0u8; 256];
    for (i, &c) in alphabet.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == b'=' {
            break;
        }
        let a = table[bytes[i] as usize] as u32;
        let b = table[bytes[i + 1] as usize] as u32;
        let c = if bytes[i + 2] == b'=' {
            0
        } else {
            table[bytes[i + 2] as usize] as u32
        };
        let d = if i + 3 >= bytes.len() || bytes[i + 3] == b'=' {
            0
        } else {
            table[bytes[i + 3] as usize] as u32
        };
        let triple = (a << 18) | (b << 12) | (c << 6) | d;
        out.push(((triple >> 16) & 0xFF) as u8);
        if bytes[i + 2] != b'=' {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if i + 3 < bytes.len() && bytes[i + 3] != b'=' {
            out.push((triple & 0xFF) as u8);
        }
        i += 4;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::cli::UseArgs;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

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

    fn claude_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn setup_claude_api_key_profile(home: &Path, name: &str) {
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        auth::claude::add_api_key(&ps, &cs, name, claude_key(), None).unwrap();
    }

    fn setup_gemini_api_key_profile(home: &Path, name: &str) {
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        auth::gemini::add_api_key(&ps, &cs, name, "AIzatest1234567890ABCDEF", None).unwrap();
    }

    fn use_args(tool: Tool, name: &str, emit_env: bool) -> UseArgs {
        UseArgs {
            tool: Some(tool),
            profile_name: Some(name.to_owned()),
            state_mode: None,
            emit_env,
            all: false,
            all_profile: None,
        }
    }

    #[test]
    fn nonexistent_profile_errors() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();

        let err = run_in(use_args(Tool::Claude, "ghost", false), &home, &user_home).unwrap_err();
        assert!(err.to_string().contains("not found"), "unexpected: {}", err);
    }

    #[test]
    fn typo_suggestion_did_you_mean() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        let err = run_in(use_args(Tool::Claude, "wrk", false), &home, &user_home).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Did you mean 'work'?"), "unexpected: {}", msg);
    }

    #[test]
    fn claude_api_key_emit_env_updates_active() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        // run_in with emit_env=true — output goes to stdout (captured by test runner,
        // not easily assertable here; we verify no error and config updated).
        run_in(use_args(Tool::Claude, "work", true), &home, &user_home).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
    }

    #[test]
    fn use_updates_active_in_config() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        run_in(use_args(Tool::Claude, "work", false), &home, &user_home).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
    }

    #[test]
    fn use_creates_backup_when_enabled() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        setup_claude_api_key_profile(&home, "work");

        run_in(use_args(Tool::Claude, "work", false), &home, &user_home).unwrap();

        let backups_dir = home.join("backups");
        assert!(backups_dir.exists(), "backups dir should be created");
        let entries: Vec<_> = fs::read_dir(&backups_dir).unwrap().collect();
        assert!(!entries.is_empty(), "at least one backup entry expected");
    }

    #[test]
    fn gemini_api_key_writes_env_file() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        setup_gemini_api_key_profile(&home, "work");

        run_in(use_args(Tool::Gemini, "work", false), &home, &user_home).unwrap();

        let env_file = user_home.join(".gemini").join(".env");
        assert!(env_file.exists(), ".env should be written to gemini dir");
        let contents = fs::read_to_string(&env_file).unwrap();
        assert!(contents.contains("GEMINI_API_KEY="));
    }

    #[test]
    fn codex_api_key_emit_env_updates_active() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        fs::create_dir_all(&home).unwrap();
        let ps = ProfileStore::new(&home);
        let cs = ConfigStore::new(&home);
        auth::codex::add_api_key(&ps, &cs, "work", "sk-codex-test-key-12345", None).unwrap();

        run_in(use_args(Tool::Codex, "work", true), &home, &user_home).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(config.active_for(Tool::Codex), Some("work"));
    }

    // ---- extract_switch_identity tests ----

    #[test]
    fn identity_extracted_from_claude_oauth_account_email() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let ps = ProfileStore::new(home);
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            ".credentials.json",
            br#"{"oauthToken":"tok","account":{"email":"work@example.com"}}"#,
        )
        .unwrap();

        let identity = extract_switch_identity(&ps, Tool::Claude, "work");
        assert_eq!(identity.as_deref(), Some("work@example.com"));
    }

    #[test]
    fn identity_extracted_from_claude_oauth_account_metadata() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let ps = ProfileStore::new(home);
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            ".credentials.json",
            br#"{"claudeAiOauth":{"accessToken":"tok"},"oauthAccount":{"emailAddress":"team@example.com"}}"#,
        )
        .unwrap();

        let identity = extract_switch_identity(&ps, Tool::Claude, "work");
        assert_eq!(identity.as_deref(), Some("team@example.com"));
    }

    #[test]
    fn identity_none_for_api_key_profile() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        auth::claude::add_api_key(&ps, &cs, "work", claude_key(), None).unwrap();

        // API key JSON has no email field — should return None, not error.
        let identity = extract_switch_identity(&ps, Tool::Claude, "work");
        assert!(identity.is_none());
    }

    #[test]
    fn identity_none_when_cred_file_missing() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let ps = ProfileStore::new(home);
        ps.create(Tool::Claude, "ghost").unwrap();
        // No credential file written.
        let identity = extract_switch_identity(&ps, Tool::Claude, "ghost");
        assert!(identity.is_none());
    }

    #[test]
    fn identity_extracted_from_codex_account_email() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let ps = ProfileStore::new(home);
        ps.create(Tool::Codex, "work").unwrap();
        ps.write_file(
            Tool::Codex,
            "work",
            "auth.json",
            br#"{"account":{"email":"dev@example.com"}}"#,
        )
        .unwrap();

        let identity = extract_switch_identity(&ps, Tool::Codex, "work");
        assert_eq!(identity.as_deref(), Some("dev@example.com"));
    }

    #[test]
    fn decode_jwt_email_extracts_email_claim() {
        // Craft a minimal JWT with a known payload: {"email":"user@example.com"}
        // Header: {"alg":"HS256"} → eyJhbGciOiJIUzI1NiJ9
        // Payload: {"email":"user@example.com"} → encode manually
        let payload = r#"{"email":"user@example.com"}"#;
        let b64 = base64_encode(payload.as_bytes());
        let fake_jwt = format!("eyJhbGciOiJIUzI1NiJ9.{b64}.signature");
        let email = decode_jwt_email(&fake_jwt);
        assert_eq!(email.as_deref(), Some("user@example.com"));
    }

    fn base64_encode(input: &[u8]) -> String {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let triple = (b0 << 16) | (b1 << 8) | b2;
            out.push(alphabet[((triple >> 18) & 0x3F) as usize] as char);
            out.push(alphabet[((triple >> 12) & 0x3F) as usize] as char);
            if chunk.len() > 1 {
                out.push(alphabet[((triple >> 6) & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(alphabet[(triple & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    fn setup_codex_api_key_profile(home: &Path, name: &str) {
        let ps = ProfileStore::new(home);
        let cs = ConfigStore::new(home);
        auth::codex::add_api_key(&ps, &cs, name, "sk-codex-test-key-12345", None).unwrap();
    }

    #[test]
    fn all_flag_switches_all_tools_with_profile() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&user_home).unwrap();
        setup_claude_api_key_profile(&home, "work");
        setup_codex_api_key_profile(&home, "work");
        setup_gemini_api_key_profile(&home, "work");

        run_all_in("work", &home, &user_home).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
        assert_eq!(config.active_for(Tool::Codex), Some("work"));
        assert_eq!(config.active_for(Tool::Gemini), Some("work"));
    }

    #[test]
    fn all_flag_skips_tools_without_profile() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&user_home).unwrap();
        setup_claude_api_key_profile(&home, "work");
        // Only Claude has "work"

        run_all_in("work", &home, &user_home).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
        assert_eq!(config.active_for(Tool::Codex), None);
        assert_eq!(config.active_for(Tool::Gemini), None);
    }

    #[test]
    fn all_flag_errors_when_no_tool_has_profile() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let user_home = tmp.path().join("uhome");
        std::fs::create_dir_all(&home).unwrap();

        let err = run_all_in("work", &home, &user_home).unwrap_err();
        assert!(
            err.to_string().contains("no tool has a profile"),
            "unexpected: {}",
            err
        );
    }
}
