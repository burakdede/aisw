use std::ffi::OsString;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::auth;
use crate::auth::identity;
use crate::cli::AddArgs;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::output;
use crate::profile::ProfileStore;
use crate::runtime;
use crate::tool_detection;
use crate::types::Tool;

pub const CLAUDE_ENV_VAR: &str = "ANTHROPIC_API_KEY";
pub const CODEX_ENV_VAR: &str = "OPENAI_API_KEY";
pub const GEMINI_ENV_VAR: &str = "GEMINI_API_KEY";

pub fn run(args: AddArgs, home: &Path) -> Result<()> {
    run_in(args, home, std::env::var_os("PATH").unwrap_or_default())
}

pub(crate) fn run_in(args: AddArgs, home: &Path, tool_path: OsString) -> Result<()> {
    // --from-live captures live credentials without launching any login flow.
    // Tool detection is intentionally skipped: the tool is already installed
    // and logged in, which is the prerequisite for --from-live to succeed.
    if args.from_live {
        let user_home = dirs::home_dir().context("could not determine home directory")?;
        return from_live(args, home, &user_home);
    }

    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;

    if profile_store.exists(args.tool, &args.profile_name)
        && !config
            .profiles_for(args.tool)
            .contains_key(&args.profile_name)
    {
        profile_store.delete(args.tool, &args.profile_name)?;
    }

    // Guard: tool binary must be on PATH before we create any profile state.
    let detected = tool_detection::require_in(args.tool, tool_path)?;

    if args.from_env {
        let env_var = match args.tool {
            Tool::Claude => CLAUDE_ENV_VAR,
            Tool::Codex => CODEX_ENV_VAR,
            Tool::Gemini => GEMINI_ENV_VAR,
        };
        let key = std::env::var(env_var).unwrap_or_default();
        if key.is_empty() {
            anyhow::bail!("{} is not set — cannot use --from-env", env_var);
        }
        match args.tool {
            Tool::Claude => auth::claude::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                &key,
                args.label.clone(),
            )?,
            Tool::Codex => auth::codex::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                &key,
                args.label.clone(),
            )?,
            Tool::Gemini => auth::gemini::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                &key,
                args.label.clone(),
            )?,
        }
        if args.set_active {
            config_store.set_active(args.tool, &args.profile_name)?;
        }
        output::print_title("Added profile");
        output::print_kv("Tool", args.tool.display_name());
        output::print_kv("Profile", &args.profile_name);
        output::print_kv("Source", env_var);
        output::print_kv(
            "Activation",
            if args.set_active { "active" } else { "stored" },
        );
        output::print_blank_line();
        output::print_effects_header();
        output::print_effect("Profile credentials stored in aisw.");
        output::print_blank_line();
        output::print_next_step(output::next_step_after_add(
            args.tool,
            &args.profile_name,
            args.set_active,
        ));
        return Ok(());
    }

    if let Some(ref api_key) = args.api_key {
        match args.tool {
            Tool::Claude => auth::claude::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
            )?,
            Tool::Codex => auth::codex::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
            )?,
            Tool::Gemini => auth::gemini::add_api_key(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
            )?,
        }
    } else {
        if runtime::is_non_interactive() {
            anyhow::bail!(
                "{} requires interactive authentication when --api-key is not provided.\n  \
                 Re-run without --non-interactive, or pass --api-key.",
                args.tool.display_name()
            );
        }
        match args.tool {
            Tool::Claude => {
                let (live_snapshot, oauth_account_snapshot, user_home): (
                    Option<auth::claude::LiveCredentialSnapshot>,
                    Option<Vec<u8>>,
                    Option<std::path::PathBuf>,
                ) = if args.set_active {
                    (None, None, None)
                } else {
                    let user_home =
                        dirs::home_dir().context("could not determine home directory")?;
                    (
                        auth::claude::live_credentials_snapshot_for_import(&user_home)?,
                        auth::claude::read_live_oauth_account_metadata_for_import(&user_home)?,
                        Some(user_home),
                    )
                };
                auth::claude::add_oauth(
                    &profile_store,
                    &config_store,
                    &args.profile_name,
                    args.label.clone(),
                    &detected.binary_path,
                )?;
                if let Some(user_home) = user_home.as_deref() {
                    // `add` should only store a profile; it must not switch the
                    // current live Claude account unless --set-active is requested.
                    auth::claude::restore_live_state_after_oauth_add(
                        live_snapshot,
                        oauth_account_snapshot,
                        user_home,
                    )?;
                }
            }
            Tool::Codex => auth::codex::add_oauth(
                &profile_store,
                &config_store,
                &args.profile_name,
                args.label.clone(),
                &detected.binary_path,
            )?,
            Tool::Gemini => auth::gemini::add_oauth(
                &profile_store,
                &config_store,
                &args.profile_name,
                args.label.clone(),
                &detected.binary_path,
            )?,
        }
    }

    if args.set_active {
        config_store.set_active(args.tool, &args.profile_name)?;
    }

    output::print_title("Added profile");
    output::print_kv("Tool", args.tool.display_name());
    output::print_kv("Profile", &args.profile_name);
    output::print_kv(
        "Activation",
        if args.set_active { "active" } else { "stored" },
    );
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Profile credentials stored in aisw.");
    if args.set_active {
        output::print_effect("Active profile updated.");
    }
    output::print_blank_line();
    output::print_next_step(output::next_step_after_add(
        args.tool,
        &args.profile_name,
        args.set_active,
    ));

    Ok(())
}

// ---- --from-live implementation --------------------------------------------

fn confirm_overwrite(tool: Tool, name: &str, yes: bool) -> Result<()> {
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

fn prepare_from_live_target(
    profile_store: &ProfileStore,
    tool: Tool,
    name: &str,
    yes: bool,
) -> Result<bool> {
    let overwriting = profile_store.exists(tool, name);
    if overwriting {
        confirm_overwrite(tool, name, yes)?;
    } else {
        profile_store.create(tool, name)?;
    }
    Ok(overwriting)
}

fn from_live(args: AddArgs, home: &Path, user_home: &Path) -> Result<()> {
    match args.tool {
        Tool::Claude => from_live_claude(args, home, user_home),
        Tool::Codex => from_live_codex(args, home, user_home),
        Tool::Gemini => from_live_gemini(args, home, user_home),
    }
}

fn from_live_claude(args: AddArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    let snapshot =
        auth::claude::live_credentials_snapshot_for_import(user_home)?.with_context(|| {
            format!(
                "no live credentials found — run 'claude login' first, \
                 then retry 'aisw add claude {} --from-live'.",
                args.profile_name,
            )
        })?;

    let stored_backend = auth::claude::preferred_import_backend(&snapshot.source);
    let overwriting =
        prepare_from_live_target(&profile_store, Tool::Claude, &args.profile_name, args.yes)?;

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
        if !overwriting {
            let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        }
        if !overwriting && stored_backend == CredentialBackend::SystemKeyring {
            let _ =
                crate::auth::secure_store::delete_profile_secret(Tool::Claude, &args.profile_name);
        }
        return Err(e);
    }

    if let Err(e) = auth::claude::capture_live_oauth_account_metadata(
        &profile_store,
        &args.profile_name,
        user_home,
    ) {
        if !overwriting {
            let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        }
        return Err(e);
    }

    if let Err(e) = identity::ensure_unique_oauth_identity(
        &profile_store,
        &config_store,
        Tool::Claude,
        &args.profile_name,
        stored_backend,
    ) {
        if !overwriting {
            let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        }
        if !overwriting && stored_backend == CredentialBackend::SystemKeyring {
            let _ =
                crate::auth::secure_store::delete_profile_secret(Tool::Claude, &args.profile_name);
        }
        return Err(e);
    }

    let add_result = if overwriting {
        config_store.upsert_profile(
            Tool::Claude,
            &args.profile_name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: stored_backend,
                label: args.label.clone(),
            },
        )
    } else {
        config_store.add_profile(
            Tool::Claude,
            &args.profile_name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: stored_backend,
                label: args.label.clone(),
            },
        )
    };

    if let Err(e) = add_result {
        if !overwriting {
            let _ = profile_store.delete(Tool::Claude, &args.profile_name);
        }
        if !overwriting && stored_backend == CredentialBackend::SystemKeyring {
            let _ =
                crate::auth::secure_store::delete_profile_secret(Tool::Claude, &args.profile_name);
        }
        return Err(e);
    }

    auth::claude::apply_live_credentials(
        &profile_store,
        &args.profile_name,
        stored_backend,
        user_home,
    )?;
    config_store.activate_profile(Tool::Claude, &args.profile_name, None)?;

    finalize_from_live(&args, Tool::Claude, stored_backend, AuthMethod::OAuth)
}

fn from_live_codex(args: AddArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    let snapshot =
        auth::codex::live_credentials_snapshot_for_import(user_home)?.with_context(|| {
            format!(
                "no live credentials found — run 'codex login' first, \
                 then retry 'aisw add codex {} --from-live'.",
                args.profile_name,
            )
        })?;

    let is_api_key = json_string_field(&snapshot.bytes, "token").is_some();
    let auth_method = if is_api_key {
        AuthMethod::ApiKey
    } else {
        AuthMethod::OAuth
    };

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

    let overwriting =
        prepare_from_live_target(&profile_store, Tool::Codex, &args.profile_name, args.yes)?;

    if let Err(e) = auth::codex::write_file_store_config(&profile_store, &args.profile_name) {
        if !overwriting {
            let _ = profile_store.delete(Tool::Codex, &args.profile_name);
        }
        return Err(e);
    }

    if let Err(e) = profile_store.write_file(
        Tool::Codex,
        &args.profile_name,
        auth::codex::AUTH_FILE,
        &snapshot.bytes,
    ) {
        if !overwriting {
            let _ = profile_store.delete(Tool::Codex, &args.profile_name);
        }
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
            if !overwriting {
                let _ = profile_store.delete(Tool::Codex, &args.profile_name);
            }
            return Err(e);
        }
    }

    let add_result = if overwriting {
        config_store.upsert_profile(
            Tool::Codex,
            &args.profile_name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method,
                credential_backend: CredentialBackend::File,
                label: args.label.clone(),
            },
        )
    } else {
        config_store.add_profile(
            Tool::Codex,
            &args.profile_name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method,
                credential_backend: CredentialBackend::File,
                label: args.label.clone(),
            },
        )
    };

    if let Err(e) = add_result {
        if !overwriting {
            let _ = profile_store.delete(Tool::Codex, &args.profile_name);
        }
        return Err(e);
    }

    auth::codex::apply_live_files(&profile_store, &args.profile_name, user_home)?;
    config_store.activate_profile(Tool::Codex, &args.profile_name, None)?;

    finalize_from_live(&args, Tool::Codex, CredentialBackend::File, auth_method)
}

fn from_live_gemini(args: AddArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    let selection = auth::gemini::detect_live_import_selection(user_home)?.with_context(|| {
        format!(
            "no live credentials found in {} — run 'gemini login' first, \
             then retry 'aisw add gemini {} --from-live'.",
            auth::gemini::live_dir(user_home).display(),
            args.profile_name,
        )
    })?;
    let gemini_dir = auth::gemini::live_dir(user_home);
    let env_file = selection.env_file;
    let oauth_files = selection.oauth_files;
    let auth_method = selection.method;

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

    let overwriting =
        prepare_from_live_target(&profile_store, Tool::Gemini, &args.profile_name, args.yes)?;

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
        if !overwriting {
            let _ = profile_store.delete(Tool::Gemini, &args.profile_name);
        }
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
            if !overwriting {
                let _ = profile_store.delete(Tool::Gemini, &args.profile_name);
            }
            return Err(e);
        }
    }

    let add_result = if overwriting {
        config_store.upsert_profile(
            Tool::Gemini,
            &args.profile_name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method,
                credential_backend: CredentialBackend::File,
                label: args.label.clone(),
            },
        )
    } else {
        config_store.add_profile(
            Tool::Gemini,
            &args.profile_name,
            ProfileMeta {
                added_at: Utc::now(),
                auth_method,
                credential_backend: CredentialBackend::File,
                label: args.label.clone(),
            },
        )
    };

    if let Err(e) = add_result {
        if !overwriting {
            let _ = profile_store.delete(Tool::Gemini, &args.profile_name);
        }
        return Err(e);
    }

    match auth_method {
        AuthMethod::OAuth => {
            auth::gemini::apply_token_cache(&profile_store, &args.profile_name, &gemini_dir)?
        }
        AuthMethod::ApiKey => auth::gemini::apply_env_file(
            &profile_store,
            &args.profile_name,
            &gemini_dir.join(".env"),
        )?,
    }
    config_store.activate_profile(Tool::Gemini, &args.profile_name, None)?;

    if selection.has_both_sources {
        output::print_info(
            "Both Gemini API key (.env) and OAuth cache were found. Imported .env by precedence.",
        );
    }
    finalize_from_live(&args, Tool::Gemini, CredentialBackend::File, auth_method)
}

fn finalize_from_live(
    args: &AddArgs,
    tool: Tool,
    backend: CredentialBackend,
    auth_method: AuthMethod,
) -> Result<()> {
    output::print_title("Added profile");
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
    output::print_kv("Activation", "active");
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Profile credentials stored in aisw.");
    output::print_effect("Live tool configuration updated.");
    output::print_effect("Active profile updated.");
    output::print_blank_line();
    output::print_next_step(output::next_step_after_add(tool, &args.profile_name, true));
    Ok(())
}

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

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    struct RuntimeGuard {
        non_interactive: bool,
        quiet: bool,
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
        let _spawn_lock = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _lock = env_lock().lock().unwrap();
        f()
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::remove_var(key) };
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

    impl RuntimeGuard {
        fn set(non_interactive: bool, quiet: bool) -> Self {
            let previous = Self {
                non_interactive: crate::runtime::is_non_interactive(),
                quiet: crate::runtime::is_quiet(),
            };
            crate::runtime::configure(non_interactive, quiet);
            previous
        }
    }

    impl Drop for RuntimeGuard {
        fn drop(&mut self) {
            crate::runtime::configure(self.non_interactive, self.quiet);
        }
    }

    fn make_fake_binary(dir: &Path, name: &str) {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\necho 'fake 1.0'\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn make_claude_oauth_binary(dir: &Path) {
        let path = dir.join("claude");
        fs::write(
            &path,
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n\
               echo 'claude 2.3.0'\n\
               exit 0\n\
             fi\n\
             [ \"$1\" = \"auth\" ] || exit 9\n\
             [ \"$2\" = \"login\" ] || exit 8\n\
             mkdir -p \"$HOME/.claude\"\n\
             printf '%s' '{\"oauthToken\":\"new-token\",\"account\":{\"email\":\"new@example.com\"}}' > \"$HOME/.claude/.credentials.json\"\n\
             printf '%s' '{\"oauthAccount\":{\"emailAddress\":\"new@example.com\"}}' > \"$HOME/.claude.json\"\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn path_of(dir: &Path) -> OsString {
        dir.as_os_str().to_owned()
    }

    fn claude_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn add_args_api_key(tool: Tool, name: &str, api_key: &str) -> AddArgs {
        AddArgs {
            tool,
            profile_name: name.to_owned(),
            api_key: Some(api_key.to_owned()),
            label: None,
            set_active: false,
            from_env: false,
            from_live: false,
            yes: false,
        }
    }

    fn from_live_args(tool: Tool, name: &str) -> AddArgs {
        AddArgs {
            tool,
            profile_name: name.to_owned(),
            api_key: None,
            label: None,
            set_active: false,
            from_env: false,
            from_live: true,
            yes: true,
        }
    }

    #[test]
    fn tool_not_found_errors() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let args = add_args_api_key(Tool::Claude, "work", claude_key());
        let err = run_in(args, &home, path_of(&bin_dir)).unwrap_err();
        assert!(
            err.to_string().contains("not installed") || err.to_string().contains("not found"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn api_key_claude_creates_profile() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let args = add_args_api_key(Tool::Claude, "work", claude_key());
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert!(config.profiles_for(Tool::Claude).contains_key("work"));
    }

    #[test]
    fn set_active_marks_profile_active() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let mut args = add_args_api_key(Tool::Claude, "work", claude_key());
        args.set_active = true;
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(config.active_for(Tool::Claude), Some("work"));
    }

    #[test]
    fn label_stored_in_config() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let mut args = add_args_api_key(Tool::Claude, "work", claude_key());
        args.label = Some("My work account".to_owned());
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].label.as_deref(),
            Some("My work account")
        );
    }

    #[test]
    fn invalid_profile_name_errors() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "claude");

        let args = add_args_api_key(Tool::Claude, "my profile", claude_key());
        assert!(run_in(args, &home, path_of(&bin_dir)).is_err());
    }

    #[test]
    fn codex_api_key_creates_profile() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "codex");

        let args = add_args_api_key(Tool::Codex, "work", "sk-codex-test-key-12345");
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert!(config.profiles_for(Tool::Codex).contains_key("work"));
    }

    #[test]
    fn gemini_api_key_creates_profile() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        make_fake_binary(&bin_dir, "gemini");

        let args = add_args_api_key(Tool::Gemini, "work", "AIzatest1234567890ABCDEF");
        run_in(args, &home, path_of(&bin_dir)).unwrap();

        let config = ConfigStore::new(&home).load().unwrap();
        assert!(config.profiles_for(Tool::Gemini).contains_key("work"));
    }

    fn from_env_args(tool: Tool, name: &str) -> AddArgs {
        AddArgs {
            tool,
            profile_name: name.to_owned(),
            api_key: None,
            label: None,
            set_active: false,
            from_env: true,
            from_live: false,
            yes: false,
        }
    }

    #[test]
    fn from_env_claude_creates_profile() {
        with_env_lock(|| {
            let _key = EnvVarGuard::set(
                "ANTHROPIC_API_KEY",
                "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            );
            let tmp = tempdir().unwrap();
            let home = tmp.path().join("home");
            let bin_dir = tmp.path().join("bin");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&bin_dir).unwrap();
            make_fake_binary(&bin_dir, "claude");

            run_in(from_env_args(Tool::Claude, "ci"), &home, path_of(&bin_dir)).unwrap();

            let config = ConfigStore::new(&home).load().unwrap();
            assert!(config.profiles_for(Tool::Claude).contains_key("ci"));
        });
    }

    #[test]
    fn from_env_codex_creates_profile() {
        with_env_lock(|| {
            let _key = EnvVarGuard::set("OPENAI_API_KEY", "sk-codex-test-key-12345");
            let tmp = tempdir().unwrap();
            let home = tmp.path().join("home");
            let bin_dir = tmp.path().join("bin");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&bin_dir).unwrap();
            make_fake_binary(&bin_dir, "codex");

            run_in(from_env_args(Tool::Codex, "ci"), &home, path_of(&bin_dir)).unwrap();

            let config = ConfigStore::new(&home).load().unwrap();
            assert!(config.profiles_for(Tool::Codex).contains_key("ci"));
        });
    }

    #[test]
    fn from_env_gemini_creates_profile() {
        with_env_lock(|| {
            let _key = EnvVarGuard::set("GEMINI_API_KEY", "AIzatest1234567890ABCDEF");
            let tmp = tempdir().unwrap();
            let home = tmp.path().join("home");
            let bin_dir = tmp.path().join("bin");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&bin_dir).unwrap();
            make_fake_binary(&bin_dir, "gemini");

            run_in(from_env_args(Tool::Gemini, "ci"), &home, path_of(&bin_dir)).unwrap();

            let config = ConfigStore::new(&home).load().unwrap();
            assert!(config.profiles_for(Tool::Gemini).contains_key("ci"));
        });
    }

    #[test]
    fn from_env_unset_errors() {
        with_env_lock(|| {
            let _key = EnvVarGuard::unset("ANTHROPIC_API_KEY");
            let tmp = tempdir().unwrap();
            let home = tmp.path().join("home");
            let bin_dir = tmp.path().join("bin");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&bin_dir).unwrap();
            make_fake_binary(&bin_dir, "claude");

            let err =
                run_in(from_env_args(Tool::Claude, "ci"), &home, path_of(&bin_dir)).unwrap_err();
            assert!(
                err.to_string().contains("ANTHROPIC_API_KEY"),
                "unexpected: {}",
                err
            );
        });
    }

    #[test]
    fn from_env_empty_errors() {
        with_env_lock(|| {
            let _key = EnvVarGuard::set("ANTHROPIC_API_KEY", "");
            let tmp = tempdir().unwrap();
            let home = tmp.path().join("home");
            let bin_dir = tmp.path().join("bin");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&bin_dir).unwrap();
            make_fake_binary(&bin_dir, "claude");

            let err =
                run_in(from_env_args(Tool::Claude, "ci"), &home, path_of(&bin_dir)).unwrap_err();
            assert!(
                err.to_string().contains("ANTHROPIC_API_KEY"),
                "unexpected: {}",
                err
            );
        });
    }

    #[test]
    fn claude_oauth_add_without_set_active_restores_live_state() {
        with_env_lock(|| {
            let tmp = tempdir().unwrap();
            let aisw_home = tmp.path().join("aisw-home");
            let user_home = tmp.path().join("user-home");
            let bin_dir = tmp.path().join("bin");
            fs::create_dir_all(&aisw_home).unwrap();
            fs::create_dir_all(&user_home).unwrap();
            fs::create_dir_all(&bin_dir).unwrap();
            make_claude_oauth_binary(&bin_dir);

            fs::create_dir_all(user_home.join(".claude")).unwrap();
            fs::write(
                user_home.join(".claude").join(".credentials.json"),
                r#"{"oauthToken":"old-token","account":{"email":"old@example.com"}}"#,
            )
            .unwrap();
            fs::write(
                user_home.join(".claude.json"),
                r#"{"oauthAccount":{"emailAddress":"old@example.com"}}"#,
            )
            .unwrap();

            let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
            let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

            let args = AddArgs {
                tool: Tool::Claude,
                profile_name: "work".to_owned(),
                api_key: None,
                label: None,
                set_active: false,
                from_env: false,
                from_live: false,
                yes: false,
            };
            run_in(args, &aisw_home, path_of(&bin_dir)).unwrap();

            let config = ConfigStore::new(&aisw_home).load().unwrap();
            assert_eq!(config.active_for(Tool::Claude), None);

            let stored = ProfileStore::new(&aisw_home)
                .read_file(Tool::Claude, "work", ".credentials.json")
                .unwrap();
            let stored_json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
            assert_eq!(stored_json["oauthToken"], "new-token");

            let live_credentials =
                fs::read_to_string(user_home.join(".claude").join(".credentials.json")).unwrap();
            let live_json: serde_json::Value = serde_json::from_str(&live_credentials).unwrap();
            assert_eq!(live_json["oauthToken"], "old-token");

            let live_metadata = fs::read_to_string(user_home.join(".claude.json")).unwrap();
            let metadata_json: serde_json::Value = serde_json::from_str(&live_metadata).unwrap();
            assert_eq!(
                metadata_json["oauthAccount"]["emailAddress"],
                "old@example.com"
            );
        });
    }

    // ---- --from-live tests -------------------------------------------------

    fn write_claude_credentials(user_home: &Path, token: &str) {
        let claude_dir = user_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let creds = claude_dir.join(".credentials.json");
        let content =
            format!(r#"{{"oauthToken":"{token}","account":{{"email":"test@example.com"}}}}"#);
        fs::write(&creds, content).unwrap();
        fs::set_permissions(&creds, fs::Permissions::from_mode(0o600)).unwrap();
    }

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

    fn write_gemini_oauth(user_home: &Path) {
        let gemini_dir = user_home.join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();
        let creds = gemini_dir.join("oauth_creds.json");
        fs::write(&creds, r#"{"token":"gemini-tok","expiry":"2099-01-01"}"#).unwrap();
        fs::set_permissions(&creds, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn write_gemini_env(user_home: &Path, key: &str) {
        let gemini_dir = user_home.join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();
        let env = gemini_dir.join(".env");
        fs::write(&env, format!("GEMINI_API_KEY={key}\n")).unwrap();
        fs::set_permissions(&env, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn from_live_claude_creates_profile() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_claude_credentials(&user_home, "tok-abc");
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

        run_in(
            from_live_args(Tool::Claude, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Claude, "work"));
        let stored = ps
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(json["oauthToken"], "tok-abc");
    }

    #[test]
    fn from_live_claude_always_activates() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_claude_credentials(&user_home, "tok-xyz");
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

        run_in(
            from_live_args(Tool::Claude, "personal"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert!(config.profiles_for(Tool::Claude).contains_key("personal"));
        assert_eq!(config.active_for(Tool::Claude), Some("personal"));
    }

    #[test]
    fn from_live_claude_overwrites_with_yes() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

        write_claude_credentials(&user_home, "tok-v1");
        run_in(
            from_live_args(Tool::Claude, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        write_claude_credentials(&user_home, "tok-v2");
        run_in(
            from_live_args(Tool::Claude, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let ps = ProfileStore::new(&aisw_home);
        let stored = ps
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(json["oauthToken"], "tok-v2");
    }

    #[test]
    fn from_live_claude_fails_without_credentials() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

        let err = run_in(
            from_live_args(Tool::Claude, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("no live credentials"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn from_live_codex_creates_profile() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_codex_credentials(&user_home, "codex-tok");
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        run_in(
            from_live_args(Tool::Codex, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Codex, "work"));
        let stored = ps.read_file(Tool::Codex, "work", "auth.json").unwrap();
        let json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(json["oauthToken"], "codex-tok");
    }

    #[test]
    fn from_live_codex_always_activates() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_codex_credentials(&user_home, "codex-reg");
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        run_in(
            from_live_args(Tool::Codex, "personal"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert!(config.profiles_for(Tool::Codex).contains_key("personal"));
        assert_eq!(config.active_for(Tool::Codex), Some("personal"));
    }

    #[test]
    fn from_live_codex_fails_without_credentials() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        let err = run_in(
            from_live_args(Tool::Codex, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("no live credentials"),
            "unexpected: {err}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn from_live_codex_overwrite_keeps_profile_when_parent_not_writable() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_codex_credentials(&user_home, "codex-v1");
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        run_in(
            from_live_args(Tool::Codex, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let codex_profiles = aisw_home.join("profiles").join(Tool::Codex.dir_name());
        let original_mode = fs::metadata(&codex_profiles).unwrap().permissions().mode() & 0o777;
        fs::set_permissions(&codex_profiles, fs::Permissions::from_mode(0o500)).unwrap();

        write_codex_credentials(&user_home, "codex-v2");
        let result = run_in(
            from_live_args(Tool::Codex, "work"),
            &aisw_home,
            OsString::new(),
        );

        fs::set_permissions(
            &codex_profiles,
            fs::Permissions::from_mode(original_mode.max(0o700)),
        )
        .unwrap();

        result.unwrap();

        let ps = ProfileStore::new(&aisw_home);
        let stored = ps.read_file(Tool::Codex, "work", "auth.json").unwrap();
        let json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(json["oauthToken"], "codex-v2");

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert!(config.profiles_for(Tool::Codex).contains_key("work"));
    }

    #[test]
    fn from_live_gemini_creates_profile() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_gemini_oauth(&user_home);
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        run_in(
            from_live_args(Tool::Gemini, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Gemini, "work"));
        assert!(ps
            .profile_dir(Tool::Gemini, "work")
            .join("oauth_creds.json")
            .exists());
    }

    #[test]
    fn confirm_overwrite_accepts_yes_without_prompt() {
        let _guard = RuntimeGuard::set(true, false);
        confirm_overwrite(Tool::Claude, "work", true).unwrap();
    }

    #[test]
    fn confirm_overwrite_errors_in_non_interactive_mode_without_yes() {
        let _guard = RuntimeGuard::set(true, false);
        let err = confirm_overwrite(Tool::Claude, "work", false).unwrap_err();
        assert!(err.to_string().contains("Re-run with --yes to overwrite"));
    }

    #[test]
    fn prepare_from_live_target_creates_new_profile_when_absent() {
        let _guard = RuntimeGuard::set(true, false);
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let overwriting = prepare_from_live_target(&ps, Tool::Codex, "work", false).unwrap();
        assert!(!overwriting);
        assert!(ps.exists(Tool::Codex, "work"));
    }

    #[test]
    fn from_live_codex_duplicate_api_key_alias_is_rejected() {
        let _guard = RuntimeGuard::set(true, false);
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let codex_dir = user_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("auth.json"),
            r#"{"token":"shared-token","kind":"api_key"}"#,
        )
        .unwrap();

        let ps = ProfileStore::new(&aisw_home);
        let cs = ConfigStore::new(&aisw_home);
        auth::codex::add_api_key(&ps, &cs, "existing", "shared-token", None).unwrap();

        let err = run_in(
            from_live_args(Tool::Codex, "alias"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("already exists as 'existing'"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn from_live_gemini_duplicate_api_key_alias_is_rejected() {
        let _guard = RuntimeGuard::set(true, false);
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        write_gemini_env(&user_home, "AIzaShared123");

        let ps = ProfileStore::new(&aisw_home);
        let cs = ConfigStore::new(&aisw_home);
        auth::gemini::add_api_key(&ps, &cs, "existing", "AIzaShared123", None).unwrap();

        let err = run_in(
            from_live_args(Tool::Gemini, "alias"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("already exists as 'existing'"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn from_live_gemini_always_activates() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_gemini_oauth(&user_home);
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        run_in(
            from_live_args(Tool::Gemini, "personal"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        let meta = &config.profiles_for(Tool::Gemini)["personal"];
        assert_eq!(meta.auth_method, crate::config::AuthMethod::OAuth);
        assert_eq!(config.active_for(Tool::Gemini), Some("personal"));
    }

    #[test]
    fn from_live_gemini_prefers_env_when_both_sources_exist() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        write_gemini_oauth(&user_home);
        write_gemini_env(&user_home, "AIza-priority-key");
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        run_in(
            from_live_args(Tool::Gemini, "priority"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps
            .profile_dir(Tool::Gemini, "priority")
            .join(".env")
            .exists());
        assert!(!ps
            .profile_dir(Tool::Gemini, "priority")
            .join("oauth_creds.json")
            .exists());
        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Gemini)["priority"].auth_method,
            crate::config::AuthMethod::ApiKey
        );
    }

    #[test]
    fn from_live_gemini_fails_without_credentials() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        let err = run_in(
            from_live_args(Tool::Gemini, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("no live credentials"),
            "unexpected: {err}"
        );
    }
}
