use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::auth;
use crate::auth::identity;
use crate::cli::SaveArgs;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::output;
use crate::profile::ProfileStore;
use crate::runtime;
use crate::types::Tool;

pub fn run(args: SaveArgs, home: &Path) -> Result<()> {
    let user_home = dirs::home_dir().context("could not determine home directory")?;
    run_in(args, home, &user_home)
}

pub(crate) fn run_in(args: SaveArgs, home: &Path, user_home: &Path) -> Result<()> {
    match args.tool {
        Tool::Claude => save_claude(args, home, user_home),
        Tool::Codex => save_codex(args, home, user_home),
        Tool::Gemini => save_gemini(args, home, user_home),
    }
}

// ---- Overwrite confirmation -------------------------------------------------

fn confirm_overwrite(tool: Tool, name: &str, yes: bool) -> Result<()> {
    let profile_store_check = false; // checked by caller
    let _ = profile_store_check;

    if yes {
        return Ok(());
    }
    if runtime::is_non_interactive() {
        bail!(
            "profile '{}' already exists for {}.\n  \
             Re-run with --yes to overwrite, or choose a different name.",
            name,
            tool,
        );
    }
    eprint!(
        "Profile '{}' already exists for {}. Overwrite? [y/N] ",
        name, tool
    );
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("could not read confirmation from stdin")?;
    if !matches!(line.trim(), "y" | "Y") {
        bail!("operation cancelled by user.");
    }
    Ok(())
}

// ---- Claude ----------------------------------------------------------------

fn save_claude(args: SaveArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    let snapshot =
        auth::claude::live_credentials_snapshot_for_import(user_home)?.with_context(|| {
            format!(
                "no live credentials found — run 'claude login' first, \
                 then retry 'aisw save claude {}'.",
                args.profile_name,
            )
        })?;

    let stored_backend = auth::claude::preferred_import_backend(&snapshot.source);

    if profile_store.exists(Tool::Claude, &args.profile_name) {
        confirm_overwrite(Tool::Claude, &args.profile_name, args.yes)?;
        config_store.remove_profile(Tool::Claude, &args.profile_name)?;
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
    }

    profile_store.create(Tool::Claude, &args.profile_name)?;

    let write_result = match stored_backend {
        CredentialBackend::File => profile_store.write_file(
            Tool::Claude,
            &args.profile_name,
            ".credentials.json",
            &snapshot.bytes,
        ),
        CredentialBackend::SystemKeyring => crate::auth::secure_store::write_profile_secret(
            Tool::Claude,
            &args.profile_name,
            &snapshot.bytes,
        ),
    };

    if let Err(e) = write_result {
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        return Err(e);
    }

    if let Err(e) = auth::claude::capture_live_oauth_account_metadata(
        &profile_store,
        &args.profile_name,
        user_home,
    ) {
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        return Err(e);
    }

    if let Err(e) = identity::ensure_unique_oauth_identity(
        &profile_store,
        &config_store,
        Tool::Claude,
        &args.profile_name,
        stored_backend,
    ) {
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        if stored_backend == CredentialBackend::SystemKeyring {
            let _ =
                crate::auth::secure_store::delete_profile_secret(Tool::Claude, &args.profile_name);
        }
        return Err(e);
    }

    if let Err(e) = config_store.add_profile(
        Tool::Claude,
        &args.profile_name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            credential_backend: stored_backend,
            label: args.label.clone(),
        },
    ) {
        let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        if stored_backend == CredentialBackend::SystemKeyring {
            let _ =
                crate::auth::secure_store::delete_profile_secret(Tool::Claude, &args.profile_name);
        }
        return Err(e);
    }

    // Apply to live and mark as active so status is immediately consistent.
    auth::claude::apply_live_credentials(
        &profile_store,
        &args.profile_name,
        stored_backend,
        user_home,
    )?;
    config_store.activate_profile(Tool::Claude, &args.profile_name, None)?;

    finalize(&args, Tool::Claude, stored_backend, AuthMethod::OAuth)
}

// ---- Codex -----------------------------------------------------------------

fn save_codex(args: SaveArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    let snapshot =
        auth::codex::live_credentials_snapshot_for_import(user_home)?.with_context(|| {
            format!(
                "no live credentials found — run 'codex login' first, \
                 then retry 'aisw save codex {}'.",
                args.profile_name,
            )
        })?;

    // A `token` field indicates API key auth; anything else is OAuth.
    let is_api_key = json_string_field(&snapshot.bytes, "token").is_some();
    let auth_method = if is_api_key {
        AuthMethod::ApiKey
    } else {
        AuthMethod::OAuth
    };

    // Deduplication: block saving an account that is already managed under a
    // different name. (Overwriting the same profile is handled separately.)
    if is_api_key {
        if let Some(secret) = json_string_field(&snapshot.bytes, "token") {
            if let Some(existing) = identity::existing_api_key_profile_for_secret(
                &profile_store,
                &config_store,
                Tool::Codex,
                &secret,
            )? {
                if existing != args.profile_name {
                    bail!(
                        "A Codex API key profile for this account already exists as '{}'.\n  \
                         Use that profile or remove it before saving another alias.",
                        existing
                    );
                }
            }
        }
    } else if let Some(existing) = identity::existing_oauth_profile_for_json_bytes(
        &profile_store,
        &config_store,
        Tool::Codex,
        &snapshot.bytes,
    )? {
        if existing != args.profile_name {
            bail!(
                "A Codex OAuth profile for this account already exists as '{}'.\n  \
                 Use that profile or remove it before saving another alias.",
                existing
            );
        }
    }

    if profile_store.exists(Tool::Codex, &args.profile_name) {
        confirm_overwrite(Tool::Codex, &args.profile_name, args.yes)?;
        config_store.remove_profile(Tool::Codex, &args.profile_name)?;
        let _ = profile_store.delete(Tool::Codex, &args.profile_name);
    }

    profile_store.create(Tool::Codex, &args.profile_name)?;

    if let Err(e) = auth::codex::write_file_store_config(&profile_store, &args.profile_name) {
        let _ = profile_store.delete(Tool::Codex, &args.profile_name);
        return Err(e);
    }

    if let Err(e) = profile_store.write_file(
        Tool::Codex,
        &args.profile_name,
        auth::codex::AUTH_FILE,
        &snapshot.bytes,
    ) {
        let _ = profile_store.delete(Tool::Codex, &args.profile_name);
        return Err(e);
    }

    if auth_method == AuthMethod::OAuth {
        if let Err(e) = identity::ensure_unique_oauth_identity(
            &profile_store,
            &config_store,
            Tool::Codex,
            &args.profile_name,
            CredentialBackend::File,
        ) {
            let _ = profile_store.delete(Tool::Codex, &args.profile_name);
            return Err(e);
        }
    }

    if let Err(e) = config_store.add_profile(
        Tool::Codex,
        &args.profile_name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method,
            credential_backend: CredentialBackend::File,
            label: args.label.clone(),
        },
    ) {
        let _ = profile_store.delete(Tool::Codex, &args.profile_name);
        return Err(e);
    }

    // Apply to live and mark as active so status is immediately consistent.
    auth::codex::apply_live_files(&profile_store, &args.profile_name, user_home)?;
    config_store.activate_profile(Tool::Codex, &args.profile_name, None)?;

    finalize(&args, Tool::Codex, CredentialBackend::File, auth_method)
}

// ---- Gemini ----------------------------------------------------------------

fn save_gemini(args: SaveArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    let gemini_dir = auth::gemini::live_dir(user_home);
    let env_file = gemini_dir.join(".env");
    let oauth_files = auth::gemini::live_oauth_files_for_import(user_home)?;

    // Determine what is present: API key (.env) takes priority, then OAuth files.
    let auth_method = if env_file.exists() {
        AuthMethod::ApiKey
    } else if !oauth_files.is_empty() {
        AuthMethod::OAuth
    } else {
        bail!(
            "no live credentials found in {} — run 'gemini login' first, \
             then retry 'aisw save gemini {}'.",
            gemini_dir.display(),
            args.profile_name,
        )
    };

    // Deduplication: block saving an account already managed under a different name.
    if auth_method == AuthMethod::ApiKey {
        let source_bytes = std::fs::read(&env_file)
            .with_context(|| format!("could not read {}", env_file.display()))?;
        if let Some(api_key) = gemini_api_key_from_env(&source_bytes) {
            if let Some(existing) = identity::existing_api_key_profile_for_secret(
                &profile_store,
                &config_store,
                Tool::Gemini,
                &api_key,
            )? {
                if existing != args.profile_name {
                    bail!(
                        "A Gemini API key profile for this account already exists as '{}'.\n  \
                         Use that profile or remove it before saving another alias.",
                        existing
                    );
                }
            }
        }
    } else if let Some(existing) = auth::gemini::existing_oauth_profile_for_live_files(
        &profile_store,
        &config_store,
        &oauth_files,
    )? {
        if existing != args.profile_name {
            bail!(
                "A Gemini OAuth profile for this account already exists as '{}'.\n  \
                 Use that profile or remove it before saving another alias.",
                existing
            );
        }
    }

    if profile_store.exists(Tool::Gemini, &args.profile_name) {
        confirm_overwrite(Tool::Gemini, &args.profile_name, args.yes)?;
        config_store.remove_profile(Tool::Gemini, &args.profile_name)?;
        let _ = profile_store.delete(Tool::Gemini, &args.profile_name);
    }

    profile_store.create(Tool::Gemini, &args.profile_name)?;

    let copy_result = if auth_method == AuthMethod::OAuth {
        auth::gemini::copy_live_oauth_files_into_profile(
            &profile_store,
            &args.profile_name,
            &oauth_files,
        )
    } else {
        profile_store.copy_file_into(Tool::Gemini, &args.profile_name, &env_file, ".env")
    };

    if let Err(e) = copy_result {
        let _ = profile_store.delete(Tool::Gemini, &args.profile_name);
        return Err(e);
    }

    if auth_method == AuthMethod::OAuth {
        if let Err(e) = identity::ensure_unique_oauth_identity(
            &profile_store,
            &config_store,
            Tool::Gemini,
            &args.profile_name,
            CredentialBackend::File,
        ) {
            let _ = profile_store.delete(Tool::Gemini, &args.profile_name);
            return Err(e);
        }
    }

    if let Err(e) = config_store.add_profile(
        Tool::Gemini,
        &args.profile_name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method,
            credential_backend: CredentialBackend::File,
            label: args.label.clone(),
        },
    ) {
        let _ = profile_store.delete(Tool::Gemini, &args.profile_name);
        return Err(e);
    }

    // Apply to live and mark as active so status is immediately consistent.
    match auth_method {
        AuthMethod::OAuth => {
            auth::gemini::apply_token_cache(&profile_store, &args.profile_name, &gemini_dir)?
        }
        AuthMethod::ApiKey => {
            auth::gemini::apply_env_file(&profile_store, &args.profile_name, &env_file)?
        }
    }
    config_store.activate_profile(Tool::Gemini, &args.profile_name, None)?;

    finalize(&args, Tool::Gemini, CredentialBackend::File, auth_method)
}

// ---- Shared output ---------------------------------------------------------

fn finalize(
    args: &SaveArgs,
    tool: Tool,
    backend: CredentialBackend,
    auth_method: AuthMethod,
) -> Result<()> {
    output::print_title("Saved profile");
    output::print_kv("Tool", tool.display_name());
    output::print_kv("Profile", &args.profile_name);
    output::print_kv(
        "Auth",
        match auth_method {
            AuthMethod::OAuth => "oauth",
            AuthMethod::ApiKey => "api_key",
        },
    );
    output::print_kv("Backend", backend.display_name());
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Profile credentials stored in aisw.");
    output::print_effect("Live tool configuration updated.");
    output::print_effect("Active profile updated.");
    output::print_blank_line();
    output::print_next_step(output::next_step_after_add(tool, &args.profile_name, true));

    Ok(())
}

// ---- Helpers ---------------------------------------------------------------

fn json_string_field(bytes: &[u8], field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    value.get(field)?.as_str().map(ToOwned::to_owned)
}

fn gemini_api_key_from_env(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("GEMINI_API_KEY=").map(ToOwned::to_owned))
}

// ---- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    fn save_args(tool: Tool, name: &str) -> SaveArgs {
        SaveArgs {
            tool,
            profile_name: name.to_owned(),
            label: None,
            yes: true, // skip interactive confirmation in tests
        }
    }

    // ---- Claude ------------------------------------------------------------

    fn write_claude_credentials(user_home: &Path, token: &str) {
        let claude_dir = user_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let creds = claude_dir.join(".credentials.json");
        let content =
            format!(r#"{{"oauthToken":"{token}","account":{{"email":"test@example.com"}}}}"#);
        fs::write(&creds, content).unwrap();
        fs::set_permissions(&creds, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn claude_save_creates_profile_from_live_credentials() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_claude_credentials(&user_home, "tok-abc");

        run_in(save_args(Tool::Claude, "work"), &aisw_home, &user_home).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Claude, "work"));
        let stored = ps
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(json["oauthToken"], "tok-abc");
    }

    #[test]
    fn claude_save_marks_profile_active() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_claude_credentials(&user_home, "tok-xyz");

        run_in(save_args(Tool::Claude, "personal"), &aisw_home, &user_home).unwrap();

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert!(config.profiles_for(Tool::Claude).contains_key("personal"));
        assert_eq!(config.active_for(Tool::Claude), Some("personal"));
    }

    #[test]
    fn claude_save_overwrites_existing_profile_with_yes() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_claude_credentials(&user_home, "tok-v1");
        run_in(save_args(Tool::Claude, "work"), &aisw_home, &user_home).unwrap();

        // Update live credentials and save again with --yes.
        write_claude_credentials(&user_home, "tok-v2");
        run_in(save_args(Tool::Claude, "work"), &aisw_home, &user_home).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        let stored = ps
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(json["oauthToken"], "tok-v2");
    }

    #[test]
    fn claude_save_fails_without_live_credentials() {
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let err = run_in(save_args(Tool::Claude, "work"), &aisw_home, &user_home).unwrap_err();
        assert!(
            err.to_string().contains("no live credentials"),
            "unexpected: {err}"
        );
    }

    // ---- Codex -------------------------------------------------------------

    fn write_codex_credentials(user_home: &Path, token: &str) {
        let codex_dir = user_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        let auth = codex_dir.join("auth.json");
        let content = format!(
            r#"{{"primaryEmail":"test@example.com","oauthToken":"{token}","refreshToken":"refresh"}}"#
        );
        fs::write(&auth, content).unwrap();
        fs::set_permissions(&auth, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn codex_save_creates_profile_from_live_credentials() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_codex_credentials(&user_home, "codex-tok");

        run_in(save_args(Tool::Codex, "work"), &aisw_home, &user_home).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Codex, "work"));

        let stored = ps.read_file(Tool::Codex, "work", "auth.json").unwrap();
        let json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(json["oauthToken"], "codex-tok");

        assert!(ps
            .profile_dir(Tool::Codex, "work")
            .join("config.toml")
            .exists());
    }

    #[test]
    fn codex_save_marks_profile_active() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_codex_credentials(&user_home, "codex-reg");

        run_in(save_args(Tool::Codex, "personal"), &aisw_home, &user_home).unwrap();

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert!(config.profiles_for(Tool::Codex).contains_key("personal"));
        assert_eq!(config.active_for(Tool::Codex), Some("personal"));
    }

    #[test]
    fn codex_save_fails_without_live_credentials() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let err = run_in(save_args(Tool::Codex, "work"), &aisw_home, &user_home).unwrap_err();
        assert!(
            err.to_string().contains("no live credentials"),
            "unexpected: {err}"
        );
    }

    // ---- Gemini OAuth ------------------------------------------------------

    fn write_gemini_oauth(user_home: &Path) {
        let gemini_dir = user_home.join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();
        let creds = gemini_dir.join("oauth_creds.json");
        fs::write(&creds, r#"{"token":"gemini-tok","expiry":"2099-01-01"}"#).unwrap();
        fs::set_permissions(&creds, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn gemini_save_creates_profile_from_oauth_files() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_gemini_oauth(&user_home);

        run_in(save_args(Tool::Gemini, "work"), &aisw_home, &user_home).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Gemini, "work"));
        assert!(ps
            .profile_dir(Tool::Gemini, "work")
            .join("oauth_creds.json")
            .exists());
    }

    #[test]
    fn gemini_save_marks_profile_active() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_gemini_oauth(&user_home);

        run_in(save_args(Tool::Gemini, "personal"), &aisw_home, &user_home).unwrap();

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        let meta = &config.profiles_for(Tool::Gemini)["personal"];
        assert_eq!(meta.auth_method, crate::config::AuthMethod::OAuth);
        assert_eq!(config.active_for(Tool::Gemini), Some("personal"));
    }

    #[test]
    fn gemini_save_fails_without_live_credentials() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        let err = run_in(save_args(Tool::Gemini, "work"), &aisw_home, &user_home).unwrap_err();
        assert!(
            err.to_string().contains("no live credentials"),
            "unexpected: {err}"
        );
    }
}
