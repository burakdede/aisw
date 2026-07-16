use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::auth;
use crate::auth::identity;
use crate::cli::{AddArgs, AddCredentialBackend};
use crate::config::{AuthMethod, Config, ConfigStore, CredentialBackend, ProfileMeta};
use crate::error::AiswError;
use crate::machine;
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
    let mut progress = machine::ProgressReporter::new(
        "add",
        Some(args.tool.binary_name()),
        Some(args.profile_name.clone()),
    );
    if let Some(progress) = progress.as_mut() {
        progress.started()?;
    }

    let requested_backend = args.credential_backend.map(map_cli_backend);
    validate_requested_backend(args.tool, requested_backend)?;
    validate_auth_source_support(&args)?;

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

    if config
        .profiles_for(args.tool)
        .contains_key(&args.profile_name)
    {
        let err = AiswError::ProfileAlreadyExists {
            tool: args.tool,
            name: args.profile_name.clone(),
        };
        if runtime::is_machine_mode() {
            return Err(err.into());
        }
        bail!("{}\n  Choose a different name.", err);
    }

    if profile_store.exists(args.tool, &args.profile_name)
        && !config
            .profiles_for(args.tool)
            .contains_key(&args.profile_name)
    {
        profile_store.delete(args.tool, &args.profile_name)?;
    }

    // Guard: tool binary must be on PATH before we create any profile state.
    let detected = tool_detection::require_in(args.tool, tool_path)?;

    let stdin_api_key = if args.api_key_stdin {
        Some(read_api_key_from_stdin()?)
    } else {
        None
    };
    let api_key_arg = args.api_key.clone().or(stdin_api_key);

    let (backend, auth_method, source) = if args.from_env {
        let backend = resolved_api_key_backend(&args);
        let env_var = match args.tool {
            Tool::Claude => CLAUDE_ENV_VAR,
            Tool::Codex => CODEX_ENV_VAR,
            Tool::Gemini => GEMINI_ENV_VAR,
            Tool::Antigravity => unreachable!("validated above"),
        };
        let key = std::env::var(env_var).unwrap_or_default();
        if key.is_empty() {
            anyhow::bail!("{} is not set — cannot use --from-env", env_var);
        }
        match args.tool {
            Tool::Claude => auth::claude::add_api_key_with_backend(
                &profile_store,
                &config_store,
                &args.profile_name,
                &key,
                args.label.clone(),
                backend,
            )?,
            Tool::Codex => auth::codex::add_api_key_with_backend(
                &profile_store,
                &config_store,
                &args.profile_name,
                &key,
                args.label.clone(),
                backend,
            )?,
            Tool::Gemini => auth::gemini::add_api_key_with_backend(
                &profile_store,
                &config_store,
                &args.profile_name,
                &key,
                args.label.clone(),
                backend,
            )?,
            Tool::Antigravity => unreachable!("validated above"),
        }
        if args.set_active {
            config_store.set_active(args.tool, &args.profile_name)?;
        }
        emit_add_result(
            &args,
            backend,
            Some(env_var),
            AuthMethod::ApiKey,
            None,
            progress.as_mut(),
        )?;
        return Ok(());
    } else if let Some(api_key) = api_key_arg.as_deref() {
        let backend = resolved_api_key_backend(&args);
        match args.tool {
            Tool::Claude => auth::claude::add_api_key_with_backend(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
                backend,
            )?,
            Tool::Codex => auth::codex::add_api_key_with_backend(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
                backend,
            )?,
            Tool::Gemini => auth::gemini::add_api_key_with_backend(
                &profile_store,
                &config_store,
                &args.profile_name,
                api_key,
                args.label.clone(),
                backend,
            )?,
            Tool::Antigravity => unreachable!("validated above"),
        }
        (backend, AuthMethod::ApiKey, None)
    } else {
        if runtime::is_non_interactive() {
            anyhow::bail!(
                "{} requires interactive authentication when --api-key is not provided.\n  \
                 Re-run without --non-interactive, or pass --api-key.",
                args.tool.display_name()
            );
        }
        if let Some(progress) = progress.as_mut() {
            progress.info("starting_upstream_auth", "Starting upstream OAuth flow")?;
            progress.waiting_for_user(
                "waiting_for_user",
                "Complete login in the browser or terminal",
                true,
            )?;
        }
        match args.tool {
            Tool::Claude => {
                let backend = requested_backend.unwrap_or_else(auth::claude::oauth_stored_backend);
                let (live_snapshot, oauth_account_snapshot, user_home): (
                    Option<auth::claude::LiveCredentialSnapshot>,
                    Option<Vec<u8>>,
                    Option<std::path::PathBuf>,
                ) = if args.set_active
                    || dirs::home_dir()
                        .as_deref()
                        .is_some_and(auth::claude::login_targets_profile_state)
                {
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
                if requested_backend.is_none() {
                    if let Some(note) = auth::claude::storage_fallback_note(
                        crate::config::CredentialBackend::SystemKeyring,
                    ) {
                        output::print_warning(note);
                    }
                }
                auth::claude::add_oauth_with_backend(
                    &profile_store,
                    &config_store,
                    &args.profile_name,
                    args.label.clone(),
                    &detected.binary_path,
                    backend,
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
                (backend, AuthMethod::OAuth, None)
            }
            Tool::Codex => {
                let backend = requested_backend.unwrap_or(CredentialBackend::File);
                auth::codex::add_oauth_with_backend(
                    &profile_store,
                    &config_store,
                    &args.profile_name,
                    args.label.clone(),
                    &detected.binary_path,
                    backend,
                )?;
                (backend, AuthMethod::OAuth, None)
            }
            Tool::Gemini => {
                auth::gemini::add_oauth(
                    &profile_store,
                    &config_store,
                    &args.profile_name,
                    args.label.clone(),
                    &detected.binary_path,
                )?;
                (CredentialBackend::File, AuthMethod::OAuth, None)
            }
            Tool::Antigravity => {
                let backend = requested_backend.unwrap_or(CredentialBackend::File);
                let user_home = dirs::home_dir().context("could not determine home directory")?;
                let live_snapshot = (!args.set_active)
                    .then(|| auth::antigravity::capture_live_snapshot(&user_home))
                    .transpose()?;
                auth::antigravity::add_oauth_with_backend(
                    &profile_store,
                    &config_store,
                    &args.profile_name,
                    args.label.clone(),
                    &detected.binary_path,
                    backend,
                )?;
                if let Some(snapshot) = live_snapshot {
                    auth::antigravity::restore_live_state_after_oauth_add(
                        Some(snapshot),
                        &user_home,
                    )?;
                }
                (backend, AuthMethod::OAuth, None)
            }
        }
    };

    if let Some(progress) = progress.as_mut() {
        progress.info("applying_changes", "Applying captured credentials")?;
    }
    if args.set_active {
        config_store.set_active(args.tool, &args.profile_name)?;
    }

    let user_home = dirs::home_dir();
    emit_add_result(
        &args,
        backend,
        source,
        auth_method,
        user_home.as_deref(),
        progress.as_mut(),
    )?;

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

struct FromLiveOverwriteSnapshot {
    config: Config,
    files: Vec<(String, Vec<u8>)>,
    secure_secret: Option<Vec<u8>>,
    secure_backend_was_tracked: bool,
}

impl FromLiveOverwriteSnapshot {
    fn capture(
        profile_store: &ProfileStore,
        config_store: &ConfigStore,
        tool: Tool,
        name: &str,
    ) -> Result<Self> {
        let config = config_store.load()?;
        let files =
            auth::files::list_regular_files_recursive(&profile_store.profile_dir(tool, name))?
                .into_iter()
                .map(|file| {
                    let bytes = fs::read(&file.path)
                        .with_context(|| format!("could not read {}", file.path.display()))?;
                    Ok((file.file_name.to_string_lossy().into_owned(), bytes))
                })
                .collect::<Result<Vec<_>>>()?;
        let old_backend = config
            .profiles_for(tool)
            .get(name)
            .map(|meta| meta.credential_backend);
        let secure_backend_was_tracked = old_backend == Some(CredentialBackend::SystemKeyring);
        let secure_secret = if secure_backend_was_tracked {
            crate::auth::secure_store::read_profile_secret(tool, name)?
        } else {
            None
        };

        Ok(Self {
            config,
            files,
            secure_secret,
            secure_backend_was_tracked,
        })
    }

    fn restore(
        &self,
        profile_store: &ProfileStore,
        config_store: &ConfigStore,
        tool: Tool,
        name: &str,
        touched_backend: CredentialBackend,
    ) {
        let _ = profile_store.delete(tool, name);
        if let Ok(dir) = profile_store.create(tool, name) {
            for (file_name, bytes) in &self.files {
                let path = dir.join(file_name);
                if path.is_symlink() {
                    continue;
                }
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::write(&path, bytes).is_ok() {
                    let _ = auth::files::set_permissions_600(&path);
                }
            }
        }

        if touched_backend == CredentialBackend::SystemKeyring || self.secure_backend_was_tracked {
            let _ = crate::auth::secure_store::delete_profile_secret(tool, name);
        }
        if let Some(secret) = &self.secure_secret {
            let _ = crate::auth::secure_store::write_profile_secret(tool, name, secret);
        }

        let _ = config_store.save(&self.config);
    }
}

fn from_live(args: AddArgs, home: &Path, user_home: &Path) -> Result<()> {
    match args.tool {
        Tool::Claude => from_live_claude(args, home, user_home),
        Tool::Codex => from_live_codex(args, home, user_home),
        Tool::Gemini => from_live_gemini(args, home, user_home),
        Tool::Antigravity => from_live_antigravity(args, home, user_home),
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

    let stored_backend = requested_backend(&args)
        .unwrap_or_else(|| auth::claude::preferred_import_backend(&snapshot.source));
    let overwriting =
        prepare_from_live_target(&profile_store, Tool::Claude, &args.profile_name, args.yes)?;
    let overwrite_snapshot = if overwriting {
        Some(FromLiveOverwriteSnapshot::capture(
            &profile_store,
            &config_store,
            Tool::Claude,
            &args.profile_name,
        )?)
    } else {
        None
    };

    let write_result = auth::claude::persist_stored_credentials(
        &profile_store,
        &args.profile_name,
        stored_backend,
        &snapshot.bytes,
    );

    if let Err(e) = write_result {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Claude,
                &args.profile_name,
                stored_backend,
            );
        } else {
            let _ = profile_store.delete(Tool::Claude, &args.profile_name);
            if stored_backend == CredentialBackend::SystemKeyring {
                let _ = crate::auth::secure_store::delete_profile_secret(
                    Tool::Claude,
                    &args.profile_name,
                );
            }
        }
        return Err(e);
    }

    if let Err(e) = auth::claude::capture_live_oauth_account_metadata(
        &profile_store,
        &args.profile_name,
        user_home,
    ) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Claude,
                &args.profile_name,
                stored_backend,
            );
        } else {
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
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Claude,
                &args.profile_name,
                stored_backend,
            );
        } else {
            let _ = profile_store.delete(Tool::Claude, &args.profile_name);
            if stored_backend == CredentialBackend::SystemKeyring {
                let _ = crate::auth::secure_store::delete_profile_secret(
                    Tool::Claude,
                    &args.profile_name,
                );
            }
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
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Claude,
                &args.profile_name,
                stored_backend,
            );
        } else {
            let _ = profile_store.delete(Tool::Claude, &args.profile_name);
            if stored_backend == CredentialBackend::SystemKeyring {
                let _ = crate::auth::secure_store::delete_profile_secret(
                    Tool::Claude,
                    &args.profile_name,
                );
            }
        }
        return Err(e);
    }

    if let Err(e) = auth::claude::apply_live_credentials(
        &profile_store,
        &args.profile_name,
        stored_backend,
        user_home,
        crate::types::StateMode::Shared,
    ) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Claude,
                &args.profile_name,
                stored_backend,
            );
        }
        return Err(e);
    }
    if let Err(e) = config_store.activate_profile(Tool::Claude, &args.profile_name, None) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Claude,
                &args.profile_name,
                stored_backend,
            );
        }
        return Err(e);
    }

    finalize_from_live(
        &args,
        Tool::Claude,
        stored_backend,
        AuthMethod::OAuth,
        Some(user_home),
    )
}

fn from_live_codex(args: AddArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);

    let backend = requested_backend(&args).unwrap_or(CredentialBackend::File);
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
    let overwrite_snapshot = if overwriting {
        Some(FromLiveOverwriteSnapshot::capture(
            &profile_store,
            &config_store,
            Tool::Codex,
            &args.profile_name,
        )?)
    } else {
        None
    };

    if let Err(e) = auth::codex::write_file_store_config(&profile_store, &args.profile_name) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Codex,
                &args.profile_name,
                CredentialBackend::File,
            );
        } else {
            let _ = profile_store.delete(Tool::Codex, &args.profile_name);
        }
        return Err(e);
    }

    let write_result = match backend {
        CredentialBackend::File => profile_store.write_file(
            Tool::Codex,
            &args.profile_name,
            auth::codex::AUTH_FILE,
            &snapshot.bytes,
        ),
        CredentialBackend::SystemKeyring => crate::auth::secure_store::write_profile_secret(
            Tool::Codex,
            &args.profile_name,
            &snapshot.bytes,
        ),
    };
    if let Err(e) = write_result {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Codex,
                &args.profile_name,
                backend,
            );
        } else {
            let _ = profile_store.delete(Tool::Codex, &args.profile_name);
            if backend == CredentialBackend::SystemKeyring {
                let _ = crate::auth::secure_store::delete_profile_secret(
                    Tool::Codex,
                    &args.profile_name,
                );
            }
        }
        return Err(e);
    }

    let marker_result = if auth_method == AuthMethod::OAuth {
        auth::codex::mark_imported_bootstrap(&profile_store, &args.profile_name)
    } else {
        auth::codex::clear_imported_bootstrap_marker(&profile_store, &args.profile_name)
    };
    if let Err(e) = marker_result {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Codex,
                &args.profile_name,
                backend,
            );
        } else {
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
            backend,
        ) {
            if let Some(snapshot) = overwrite_snapshot.as_ref() {
                snapshot.restore(
                    &profile_store,
                    &config_store,
                    Tool::Codex,
                    &args.profile_name,
                    backend,
                );
            } else {
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
                credential_backend: backend,
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
                credential_backend: backend,
                label: args.label.clone(),
            },
        )
    };

    if let Err(e) = add_result {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Codex,
                &args.profile_name,
                backend,
            );
        } else {
            let _ = profile_store.delete(Tool::Codex, &args.profile_name);
            if backend == CredentialBackend::SystemKeyring {
                let _ = crate::auth::secure_store::delete_profile_secret(
                    Tool::Codex,
                    &args.profile_name,
                );
            }
        }
        return Err(e);
    }

    if let Err(e) =
        auth::codex::apply_live_credentials(&profile_store, &args.profile_name, backend, user_home)
    {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Codex,
                &args.profile_name,
                backend,
            );
        }
        return Err(e);
    }
    if let Err(e) = config_store.activate_profile(Tool::Codex, &args.profile_name, None) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Codex,
                &args.profile_name,
                backend,
            );
        }
        return Err(e);
    }

    finalize_from_live(&args, Tool::Codex, backend, auth_method, None)
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
    let overwrite_snapshot = if overwriting {
        Some(FromLiveOverwriteSnapshot::capture(
            &profile_store,
            &config_store,
            Tool::Gemini,
            &args.profile_name,
        )?)
    } else {
        None
    };

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
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Gemini,
                &args.profile_name,
                CredentialBackend::File,
            );
        } else {
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
            if let Some(snapshot) = overwrite_snapshot.as_ref() {
                snapshot.restore(
                    &profile_store,
                    &config_store,
                    Tool::Gemini,
                    &args.profile_name,
                    CredentialBackend::File,
                );
            } else {
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
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Gemini,
                &args.profile_name,
                CredentialBackend::File,
            );
        } else {
            let _ = profile_store.delete(Tool::Gemini, &args.profile_name);
        }
        return Err(e);
    }

    let apply_result = match auth_method {
        AuthMethod::OAuth => {
            auth::gemini::apply_token_cache(&profile_store, &args.profile_name, &gemini_dir)
        }
        AuthMethod::ApiKey => auth::gemini::apply_env_file(
            &profile_store,
            &args.profile_name,
            &gemini_dir.join(".env"),
        ),
    };
    if let Err(e) = apply_result {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Gemini,
                &args.profile_name,
                CredentialBackend::File,
            );
        }
        return Err(e);
    }
    if let Err(e) = config_store.activate_profile(Tool::Gemini, &args.profile_name, None) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Gemini,
                &args.profile_name,
                CredentialBackend::File,
            );
        }
        return Err(e);
    }

    if selection.has_both_sources {
        output::print_info(
            "Both Gemini API key (.env) and OAuth cache were found. Imported .env by precedence.",
        );
    }
    finalize_from_live(
        &args,
        Tool::Gemini,
        CredentialBackend::File,
        auth_method,
        None,
    )
}

fn from_live_antigravity(args: AddArgs, home: &Path, user_home: &Path) -> Result<()> {
    let profile_store = ProfileStore::new(home);
    let config_store = ConfigStore::new(home);
    let backend = requested_backend(&args).unwrap_or(CredentialBackend::File);

    let snapshot = auth::antigravity::live_credentials_snapshot_for_import(user_home)?
        .with_context(|| {
            format!(
                "no live Antigravity credentials found — run 'agy' and sign in first, \
                 then retry 'aisw add antigravity {} --from-live'.",
                args.profile_name,
            )
        })?;

    let overwriting = prepare_from_live_target(
        &profile_store,
        Tool::Antigravity,
        &args.profile_name,
        args.yes,
    )?;
    let overwrite_snapshot = if overwriting {
        Some(FromLiveOverwriteSnapshot::capture(
            &profile_store,
            &config_store,
            Tool::Antigravity,
            &args.profile_name,
        )?)
    } else {
        None
    };

    if let Err(e) = auth::antigravity::write_profile_snapshot(
        &profile_store,
        &config_store,
        &args.profile_name,
        args.label.clone(),
        backend,
        &snapshot,
        overwriting,
    ) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Antigravity,
                &args.profile_name,
                backend,
            );
        } else {
            let _ = profile_store.delete(Tool::Antigravity, &args.profile_name);
            if backend == CredentialBackend::SystemKeyring {
                let _ = crate::auth::secure_store::delete_profile_secret(
                    Tool::Antigravity,
                    &args.profile_name,
                );
            }
        }
        return Err(e);
    }

    if let Err(e) = auth::antigravity::apply_live_credentials(
        &profile_store,
        &args.profile_name,
        backend,
        user_home,
    ) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Antigravity,
                &args.profile_name,
                backend,
            );
        }
        return Err(e);
    }
    if let Err(e) = config_store.activate_profile(Tool::Antigravity, &args.profile_name, None) {
        if let Some(snapshot) = overwrite_snapshot.as_ref() {
            snapshot.restore(
                &profile_store,
                &config_store,
                Tool::Antigravity,
                &args.profile_name,
                backend,
            );
        }
        return Err(e);
    }

    finalize_from_live(
        &args,
        Tool::Antigravity,
        backend,
        AuthMethod::OAuth,
        Some(user_home),
    )
}

fn finalize_from_live(
    args: &AddArgs,
    tool: Tool,
    backend: CredentialBackend,
    auth_method: AuthMethod,
    user_home: Option<&Path>,
) -> Result<()> {
    let warnings = add_warnings(tool, auth_method, user_home);
    let result = serde_json::json!({
        "tool": tool.binary_name(),
        "profile": args.profile_name,
        "auth_method": auth_label(auth_method),
        "credential_backend": backend.display_name(),
        "active": true,
        "source": "from_live",
        "claude_auth_classification": claude_add_classification(tool, auth_method, user_home),
        "codex_auth_classification": codex_add_classification(tool, auth_method, true),
        "antigravity_auth_classification": antigravity_add_classification(tool, auth_method),
        "warnings": warnings,
    });
    if runtime::is_progress_json() {
        if let Some(mut progress) = machine::ProgressReporter::new(
            "add",
            Some(tool.binary_name()),
            Some(args.profile_name.clone()),
        ) {
            progress.started()?;
            progress.info("applying_changes", "Applying captured credentials")?;
            progress.result(true, result)?;
            return Ok(());
        }
    } else if args.json {
        machine::print_success("add", result)?;
        return Ok(());
    }
    output::print_title("Added profile");
    output::print_kv("Tool", tool.display_name());
    output::print_kv("Profile", &args.profile_name);
    output::print_kv("Auth", auth_label(auth_method));
    output::print_kv("Backend", backend.display_name());
    if let Some(classification) = claude_add_classification(tool, auth_method, user_home) {
        output::print_kv("Claude auth", classification);
    }
    if let Some(classification) = codex_add_classification(tool, auth_method, true) {
        output::print_kv("Codex auth", classification);
    }
    if let Some(classification) = antigravity_add_classification(tool, auth_method) {
        output::print_kv("Antigravity auth", classification);
    }
    output::print_kv("Activation", "active");
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Profile credentials stored in aisw.");
    output::print_effect("Live tool configuration updated.");
    output::print_effect("Active profile updated.");
    if tool == Tool::Codex && auth_method == AuthMethod::OAuth {
        output::print_effect(
            "This Codex ChatGPT profile was imported from live state as a bootstrap session.",
        );
        output::print_effect(
            "Re-login directly inside this profile's isolated CODEX_HOME for the durable path.",
        );
    }
    if tool == Tool::Antigravity {
        output::print_effect(
            "Antigravity restores the shared live OS keyring credential and the documented ~/.gemini config roots when you switch profiles.",
        );
    }
    if let Some(warning) = add_warnings(tool, auth_method, user_home).first() {
        output::print_effect(warning);
    }
    output::print_blank_line();
    output::print_next_step(output::next_step_after_add(tool, &args.profile_name, true));
    Ok(())
}

fn validate_requested_backend(tool: Tool, requested: Option<CredentialBackend>) -> Result<()> {
    let Some(backend) = requested else {
        return Ok(());
    };
    backend.validate_for_tool(tool)?;
    if backend == CredentialBackend::SystemKeyring && !auth::system_keyring::is_usable() {
        let detail = auth::system_keyring::usability_diagnostic()
            .unwrap_or_else(|| "The system keyring is not currently usable.".to_owned());
        bail!(
            "{}\n  Cannot add {} with --credential-backend system_keyring on this machine.",
            detail,
            tool.display_name()
        );
    }
    Ok(())
}

fn validate_auth_source_support(args: &AddArgs) -> Result<()> {
    if args.tool != Tool::Antigravity {
        return Ok(());
    }
    if args.from_env {
        bail!(
            "Antigravity CLI does not document API-key or environment-variable authentication.\n  \
             Use interactive OAuth or --from-live instead."
        );
    }
    if args.api_key.is_some() || args.api_key_stdin {
        bail!(
            "Antigravity CLI support in aisw is OAuth-only because upstream documents system-keyring-backed sign-in, not API-key profile auth.\n  \
             Use 'aisw add antigravity <name>' or 'aisw add antigravity <name> --from-live'."
        );
    }
    Ok(())
}

fn resolved_api_key_backend(args: &AddArgs) -> CredentialBackend {
    requested_backend(args).unwrap_or(CredentialBackend::File)
}

fn print_add_summary(
    args: &AddArgs,
    backend: CredentialBackend,
    source: Option<&str>,
    auth_method: AuthMethod,
    user_home: Option<&Path>,
) {
    output::print_title("Added profile");
    output::print_kv("Tool", args.tool.display_name());
    output::print_kv("Profile", &args.profile_name);
    output::print_kv(
        "Auth",
        match auth_method {
            AuthMethod::OAuth => "oauth",
            AuthMethod::ApiKey => "api_key",
        },
    );
    output::print_kv("Backend", backend.display_name());
    if let Some(classification) = claude_add_classification(args.tool, auth_method, user_home) {
        output::print_kv("Claude auth", classification);
    }
    if let Some(classification) = codex_add_classification(args.tool, auth_method, false) {
        output::print_kv("Codex auth", classification);
    }
    if let Some(classification) = antigravity_add_classification(args.tool, auth_method) {
        output::print_kv("Antigravity auth", classification);
    }
    if let Some(source) = source {
        output::print_kv("Source", source);
    }
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
    if args.tool == Tool::Codex && auth_method == AuthMethod::OAuth {
        output::print_effect(
            "Codex login ran inside this profile-owned CODEX_HOME, so future refreshes stay tied to this profile.",
        );
        output::print_effect("This is the durable ChatGPT-managed Codex path.");
    }
    if args.tool == Tool::Antigravity {
        output::print_effect(
            "Antigravity OAuth is restored through the shared live OS keyring entry and the documented ~/.gemini config roots.",
        );
        output::print_effect(
            "Upstream does not currently document an isolated per-profile auth root or profile selector for Antigravity.",
        );
    }
    for warning in add_warnings(args.tool, auth_method, user_home) {
        output::print_effect(warning);
    }
    output::print_blank_line();
    output::print_next_step(output::next_step_after_add(
        args.tool,
        &args.profile_name,
        args.set_active,
    ));
}

fn emit_add_result(
    args: &AddArgs,
    backend: CredentialBackend,
    source: Option<&str>,
    auth_method: AuthMethod,
    user_home: Option<&Path>,
    progress: Option<&mut machine::ProgressReporter>,
) -> Result<()> {
    let warnings = add_warnings(args.tool, auth_method, user_home);
    let result = serde_json::json!({
        "tool": args.tool.binary_name(),
        "profile": args.profile_name,
        "auth_method": auth_label(auth_method),
        "credential_backend": backend.display_name(),
        "active": args.set_active || args.from_live,
        "source": source,
        "claude_auth_classification": claude_add_classification(args.tool, auth_method, user_home),
        "codex_auth_classification": codex_add_classification(args.tool, auth_method, args.from_live),
        "antigravity_auth_classification": antigravity_add_classification(args.tool, auth_method),
        "warnings": warnings,
    });
    if let Some(progress) = progress {
        progress.result(true, result)?;
        return Ok(());
    } else if args.json {
        machine::print_success("add", result)?;
        return Ok(());
    }

    print_add_summary(args, backend, source, auth_method, user_home);
    Ok(())
}

fn auth_label(auth_method: AuthMethod) -> &'static str {
    match auth_method {
        AuthMethod::OAuth => "oauth",
        AuthMethod::ApiKey => "api_key",
    }
}

fn codex_add_classification(
    tool: Tool,
    auth_method: AuthMethod,
    from_live: bool,
) -> Option<&'static str> {
    if tool != Tool::Codex {
        return None;
    }

    Some(match auth_method {
        AuthMethod::ApiKey => "api_key",
        AuthMethod::OAuth if from_live => "chatgpt_managed_imported_bootstrap",
        AuthMethod::OAuth => "chatgpt_managed_isolated",
    })
}

fn antigravity_add_classification(tool: Tool, auth_method: AuthMethod) -> Option<&'static str> {
    (tool == Tool::Antigravity && auth_method == AuthMethod::OAuth)
        .then_some("oauth_shared_live_keyring")
}

fn claude_add_classification(
    tool: Tool,
    auth_method: AuthMethod,
    user_home: Option<&Path>,
) -> Option<&'static str> {
    if tool != Tool::Claude {
        return None;
    }

    Some(match auth_method {
        AuthMethod::ApiKey => "api_key",
        AuthMethod::OAuth => {
            if user_home.is_some_and(auth::claude::uses_live_keychain) {
                match auth::claude::current_claude_keychain_scheme() {
                    auth::claude::ClaudeKeychainScheme::LegacyShared => {
                        "oauth_macos_keychain_shared_live"
                    }
                    auth::claude::ClaudeKeychainScheme::ScopedByConfigDir => {
                        "oauth_keychain_scoped_by_config_dir"
                    }
                    auth::claude::ClaudeKeychainScheme::Unknown => "oauth_keychain_unknown",
                }
            } else {
                "oauth_file_backed"
            }
        }
    })
}

fn add_warnings(tool: Tool, auth_method: AuthMethod, user_home: Option<&Path>) -> Vec<String> {
    if tool == Tool::Antigravity && auth_method == AuthMethod::OAuth {
        return vec![
            "Antigravity currently documents shared live OS-keyring auth, not an isolated per-profile auth root. aisw switches the live keyring-backed session and Antigravity config roots transactionally.".to_owned(),
        ];
    }
    match claude_add_classification(tool, auth_method, user_home) {
        Some("oauth_macos_keychain_shared_live") => {
            vec![
                "Claude OAuth on this install uses the legacy shared live Keychain credential, so this profile is not a durable isolated account container. Use shared mode for this profile, or prefer an API key or long-lived auth token for repeatable switching.".to_owned(),
            ]
        }
        Some("oauth_keychain_unknown") => {
            vec![
                "Claude OAuth keychain behavior could not be determined for this install. Isolated switching may not be durable unless Claude scopes credentials by CLAUDE_CONFIG_DIR.".to_owned(),
            ]
        }
        Some("oauth_keychain_scoped_by_config_dir") => {
            vec![
                "Claude login ran against this profile-owned config dir, so this install can keep OAuth refreshes tied to the profile when you use isolated mode.".to_owned(),
            ]
        }
        _ => Vec::new(),
    }
}

fn read_api_key_from_stdin() -> Result<String> {
    let mut secret = String::new();
    std::io::stdin()
        .read_to_string(&mut secret)
        .context("could not read API key from stdin")?;

    if secret.ends_with("\r\n") {
        let new_len = secret.len() - 2;
        secret.truncate(new_len);
    } else if secret.ends_with('\n') {
        secret.pop();
    }

    if secret.is_empty() {
        bail!("API key stdin input was empty after trimming trailing newline.");
    }

    Ok(secret)
}

fn requested_backend(args: &AddArgs) -> Option<CredentialBackend> {
    args.credential_backend.map(map_cli_backend)
}

fn map_cli_backend(backend: AddCredentialBackend) -> CredentialBackend {
    match backend {
        AddCredentialBackend::File => CredentialBackend::File,
        AddCredentialBackend::SystemKeyring => CredentialBackend::SystemKeyring,
    }
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
        let _lock = env_lock().lock().unwrap_or_else(|p| p.into_inner());
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
            crate::runtime::configure(non_interactive, quiet, crate::runtime::OutputMode::Human);
            previous
        }
    }

    impl Drop for RuntimeGuard {
        fn drop(&mut self) {
            crate::runtime::configure(
                self.non_interactive,
                self.quiet,
                crate::runtime::OutputMode::Human,
            );
        }
    }

    fn make_fake_binary(dir: &Path, name: &str) {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\necho 'fake 1.0'\n").unwrap();
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
            api_key_stdin: false,
            label: None,
            credential_backend: None,
            set_active: false,
            from_env: false,
            from_live: false,
            yes: false,
            json: false,
            progress_json: false,
        }
    }

    fn from_live_args(tool: Tool, name: &str) -> AddArgs {
        AddArgs {
            tool,
            profile_name: name.to_owned(),
            api_key: None,
            api_key_stdin: false,
            label: None,
            credential_backend: None,
            set_active: false,
            from_env: false,
            from_live: true,
            yes: true,
            json: false,
            progress_json: false,
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
            api_key_stdin: false,
            label: None,
            credential_backend: None,
            set_active: false,
            from_env: true,
            from_live: false,
            yes: false,
            json: false,
            progress_json: false,
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
    fn antigravity_from_env_is_rejected_before_tool_detection() {
        with_env_lock(|| {
            let tmp = tempdir().unwrap();
            let home = tmp.path().join("home");
            fs::create_dir_all(&home).unwrap();

            let err = run_in(
                from_env_args(Tool::Antigravity, "work"),
                &home,
                OsString::new(),
            )
            .unwrap_err();
            assert!(
                err.to_string()
                    .contains("does not document API-key or environment-variable authentication"),
                "unexpected: {err}"
            );
        });
    }

    #[test]
    fn antigravity_api_key_auth_is_rejected_before_tool_detection() {
        with_env_lock(|| {
            let tmp = tempdir().unwrap();
            let home = tmp.path().join("home");
            fs::create_dir_all(&home).unwrap();

            let args = add_args_api_key(Tool::Antigravity, "work", "AIza-not-supported");
            let err = run_in(args, &home, OsString::new()).unwrap_err();
            assert!(err.to_string().contains("OAuth-only"), "unexpected: {err}");
        });
    }

    #[test]
    fn claude_oauth_add_without_set_active_restores_live_state() {
        with_env_lock(|| {
            let _runtime = RuntimeGuard::set(false, false);
            let tmp = tempdir().unwrap();
            let aisw_home = tmp.path().join("aisw-home");
            let user_home = tmp.path().join("user-home");
            let bin_dir = tmp.path().join("bin");
            let keyring_dir = tmp.path().join("keychain");
            fs::create_dir_all(&aisw_home).unwrap();
            fs::create_dir_all(&user_home).unwrap();
            fs::create_dir_all(&bin_dir).unwrap();
            let claude_bin = bin_dir.join("claude");
            fs::write(
                &claude_bin,
                "#!/bin/sh\n\
                 if [ \"$1\" = \"--version\" ]; then\n\
                   echo 'claude 2.1.19'\n\
                   exit 0\n\
                 fi\n\
                 [ \"$1\" = \"auth\" ] || exit 9\n\
                 [ \"$2\" = \"login\" ] || exit 8\n\
                 item=\"$AISW_KEYRING_TEST_DIR/Claude Code-credentials/${USER:-tester}\"\n\
                 mkdir -p \"$item\"\n\
                 printf '%s' \"${USER:-tester}\" > \"$item/account\"\n\
                 printf '%s' '{\"oauthToken\":\"new-token\",\"account\":{\"email\":\"new@example.com\"}}' > \"$item/secret\"\n\
                 printf '%s' '{\"oauthAccount\":{\"emailAddress\":\"new@example.com\"}}' > \"$HOME/.claude.json\"\n\
                 exit 0\n",
            )
            .unwrap();
            fs::set_permissions(&claude_bin, fs::Permissions::from_mode(0o755)).unwrap();

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
            let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "keychain");
            let _scheme = EnvVarGuard::set("AISW_CLAUDE_KEYCHAIN_SCHEME", "shared");
            let _platform = EnvVarGuard::set("AISW_TEST_CLAUDE_PLATFORM", "macos");
            let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", keyring_dir.to_str().unwrap());
            let _user = EnvVarGuard::set("USER", "tester");
            let live_item = keyring_dir.join("Claude Code-credentials").join("tester");
            fs::create_dir_all(&live_item).unwrap();
            fs::write(live_item.join("account"), "tester").unwrap();
            fs::write(
                live_item.join("secret"),
                br#"{"oauthToken":"old-token","account":{"email":"old@example.com"}}"#,
            )
            .unwrap();

            let args = AddArgs {
                tool: Tool::Claude,
                profile_name: "work".to_owned(),
                api_key: None,
                api_key_stdin: false,
                label: None,
                credential_backend: None,
                set_active: false,
                from_env: false,
                from_live: false,
                yes: false,
                json: false,
                progress_json: false,
            };
            run_in(args, &aisw_home, path_of(&bin_dir)).unwrap();

            let config = ConfigStore::new(&aisw_home).load().unwrap();
            assert_eq!(config.active_for(Tool::Claude), None);
            let backend = config.profiles_for(Tool::Claude)["work"].credential_backend;

            let stored = match backend {
                CredentialBackend::File => ProfileStore::new(&aisw_home)
                    .read_file(Tool::Claude, "work", ".credentials.json")
                    .unwrap(),
                CredentialBackend::SystemKeyring => {
                    auth::secure_store::read_profile_secret(Tool::Claude, "work")
                        .unwrap()
                        .expect("stored Claude shared-keychain profile secret")
                }
            };
            let stored_json: serde_json::Value = serde_json::from_slice(&stored).unwrap();
            assert_eq!(stored_json["oauthToken"], "new-token");

            let live_credentials = fs::read(live_item.join("secret")).unwrap();
            let live_json: serde_json::Value = serde_json::from_slice(&live_credentials).unwrap();
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

    fn write_claude_oauth_account(user_home: &Path, email: &str, org: Option<&str>) {
        let content = match org {
            Some(org) => {
                format!(
                    r#"{{"oauthAccount":{{"emailAddress":"{email}","organizationUuid":"{org}"}}}}"#
                )
            }
            None => format!(r#"{{"oauthAccount":{{"emailAddress":"{email}"}}}}"#),
        };
        fs::write(user_home.join(".claude.json"), content).unwrap();
    }

    fn write_codex_credentials(user_home: &Path, token: &str) {
        write_codex_credentials_with_account_id(user_home, token, None);
    }

    fn write_codex_credentials_with_account_id(
        user_home: &Path,
        token: &str,
        account_id: Option<&str>,
    ) {
        let codex_dir = user_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        let auth = codex_dir.join("auth.json");
        let content = match account_id {
            Some(account_id) => format!(
                r#"{{"primaryEmail":"test@example.com","tokens":{{"account_id":"{account_id}"}},"oauthToken":"{token}","refreshToken":"refresh"}}"#
            ),
            None => format!(
                r#"{{"primaryEmail":"test@example.com","oauthToken":"{token}","refreshToken":"refresh"}}"#
            ),
        };
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

    fn write_antigravity_live_state(user_home: &Path, keyring_root: &Path, secret: &str) {
        let app_dir = user_home.join(".gemini").join("antigravity-cli");
        let shared_dir = user_home.join(".gemini").join("config");
        fs::create_dir_all(app_dir.join("cache")).unwrap();
        fs::create_dir_all(shared_dir.join("projects")).unwrap();
        fs::write(app_dir.join("settings.json"), br#"{"theme":"terminal"}"#).unwrap();
        fs::write(app_dir.join("cache").join("projects.json"), br#"{}"#).unwrap();
        fs::write(shared_dir.join("hooks.json"), br#"{}"#).unwrap();
        fs::write(shared_dir.join("projects").join("repo.json"), br#"{}"#).unwrap();

        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", keyring_root.to_str().unwrap());
        let keyring_ref = auth::antigravity::default_live_keyring_ref();
        crate::auth::system_keyring::upsert_generic_password(
            &keyring_ref.service,
            &keyring_ref.account,
            secret.as_bytes(),
        )
        .unwrap();
    }

    fn profile_meta(
        auth_method: AuthMethod,
        credential_backend: CredentialBackend,
        label: &str,
    ) -> ProfileMeta {
        ProfileMeta {
            added_at: Utc::now(),
            auth_method,
            credential_backend,
            label: Some(label.to_owned()),
        }
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
    fn from_live_claude_decodes_hex_wrapped_live_credentials() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(aisw_home.join("profiles")).unwrap();
        fs::create_dir_all(user_home.join(".claude")).unwrap();
        fs::write(
            user_home.join(".claude").join(".credentials.json"),
            b"7b226f61757468546f6b656e223a22746f6b2d686578227d",
        )
        .unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

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
        assert_eq!(json["oauthToken"], "tok-hex");
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
    fn from_live_claude_overwrite_restores_previous_profile_when_config_save_fails() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

        let ps = ProfileStore::new(&aisw_home);
        ps.create(Tool::Claude, "work").unwrap();
        ps.write_file(
            Tool::Claude,
            "work",
            ".credentials.json",
            br#"{"oauthToken":"old-token","account":{"email":"old@example.com"}}"#,
        )
        .unwrap();
        let cs = ConfigStore::new(&aisw_home);
        cs.add_profile(
            Tool::Claude,
            "work",
            profile_meta(AuthMethod::OAuth, CredentialBackend::File, "old label"),
        )
        .unwrap();

        write_claude_credentials(&user_home, "new-token");
        fs::create_dir(aisw_home.join("config.json.tmp")).unwrap();

        let err = run_in(
            from_live_args(Tool::Claude, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("config.json.tmp"),
            "unexpected error: {err:#}"
        );
        let stored = ps
            .read_file(Tool::Claude, "work", ".credentials.json")
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&stored).unwrap()["oauthToken"],
            "old-token"
        );
        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Claude)["work"].label.as_deref(),
            Some("old label")
        );
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
    fn from_live_claude_allows_same_email_with_different_org() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

        write_claude_credentials(&user_home, "tok-org-a");
        write_claude_oauth_account(&user_home, "test@example.com", Some("org-a"));
        run_in(
            from_live_args(Tool::Claude, "org-a"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        write_claude_credentials(&user_home, "tok-org-b");
        write_claude_oauth_account(&user_home, "test@example.com", Some("org-b"));
        run_in(
            from_live_args(Tool::Claude, "org-b"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Claude, "org-a"));
        assert!(ps.exists(Tool::Claude, "org-b"));
    }

    #[test]
    fn from_live_claude_rejects_same_email_when_org_is_missing() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
        let _storage = EnvVarGuard::set("AISW_CLAUDE_AUTH_STORAGE", "file");

        write_claude_credentials(&user_home, "tok-org-a");
        write_claude_oauth_account(&user_home, "test@example.com", Some("org-a"));
        run_in(
            from_live_args(Tool::Claude, "org-a"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        write_claude_credentials(&user_home, "tok-no-org");
        write_claude_oauth_account(&user_home, "test@example.com", None);
        let err = run_in(
            from_live_args(Tool::Claude, "no-org"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();

        assert!(err.to_string().contains("already exists as 'org-a'"));
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
    fn from_live_codex_allows_same_email_with_different_account_id() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        write_codex_credentials_with_account_id(
            &user_home,
            "codex-workspace",
            Some("acc-workspace"),
        );
        run_in(
            from_live_args(Tool::Codex, "workspace"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        write_codex_credentials_with_account_id(&user_home, "codex-personal", Some("acc-personal"));
        run_in(
            from_live_args(Tool::Codex, "personal"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Codex, "workspace"));
        assert!(ps.exists(Tool::Codex, "personal"));
    }

    #[test]
    fn from_live_codex_rejects_same_email_when_account_id_is_same() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        write_codex_credentials_with_account_id(&user_home, "codex-workspace", Some("acc-shared"));
        run_in(
            from_live_args(Tool::Codex, "workspace"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap();

        write_codex_credentials_with_account_id(
            &user_home,
            "codex-workspace-2",
            Some("acc-shared"),
        );
        let err = run_in(
            from_live_args(Tool::Codex, "alias"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("A Codex OAuth profile for this account already exists as 'workspace'"));
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
    fn from_live_codex_system_keyring_overwrite_restores_secret_when_config_save_fails() {
        with_env_lock(|| {
            let tmp = tempdir().unwrap();
            let aisw_home = tmp.path().join("aisw");
            let user_home = tmp.path().join("user");
            let keyring_dir = tmp.path().join("keyring");
            fs::create_dir_all(&aisw_home).unwrap();
            fs::create_dir_all(&user_home).unwrap();
            let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
            let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", keyring_dir.to_str().unwrap());

            let ps = ProfileStore::new(&aisw_home);
            ps.create(Tool::Codex, "work").unwrap();
            auth::codex::write_file_store_config(&ps, "work").unwrap();
            crate::auth::secure_store::write_profile_secret(
                Tool::Codex,
                "work",
                br#"{"primaryEmail":"old@example.com","oauthToken":"old-token"}"#,
            )
            .unwrap();
            let cs = ConfigStore::new(&aisw_home);
            cs.add_profile(
                Tool::Codex,
                "work",
                profile_meta(
                    AuthMethod::OAuth,
                    CredentialBackend::SystemKeyring,
                    "old label",
                ),
            )
            .unwrap();

            write_codex_credentials(&user_home, "new-token");
            fs::create_dir(aisw_home.join("config.json.tmp")).unwrap();
            let mut args = from_live_args(Tool::Codex, "work");
            args.credential_backend = Some(AddCredentialBackend::SystemKeyring);

            let err = run_in(args, &aisw_home, OsString::new()).unwrap_err();

            assert!(
                err.to_string().contains("config.json.tmp"),
                "unexpected error: {err:#}"
            );
            let secret = crate::auth::secure_store::read_profile_secret(Tool::Codex, "work")
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&secret).unwrap()["oauthToken"],
                "old-token"
            );
            assert!(!ps
                .profile_dir(Tool::Codex, "work")
                .join("auth.json")
                .exists());
            let config = cs.load().unwrap();
            assert_eq!(
                config.profiles_for(Tool::Codex)["work"].credential_backend,
                CredentialBackend::SystemKeyring
            );
            assert_eq!(
                config.profiles_for(Tool::Codex)["work"].label.as_deref(),
                Some("old label")
            );
        });
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
    fn from_live_gemini_overwrite_restores_previous_profile_when_config_save_fails() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("user");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());

        let ps = ProfileStore::new(&aisw_home);
        ps.create(Tool::Gemini, "work").unwrap();
        ps.write_file(Tool::Gemini, "work", ".env", b"GEMINI_API_KEY=old-key\n")
            .unwrap();
        let cs = ConfigStore::new(&aisw_home);
        cs.add_profile(
            Tool::Gemini,
            "work",
            profile_meta(AuthMethod::ApiKey, CredentialBackend::File, "old label"),
        )
        .unwrap();

        write_gemini_env(&user_home, "new-key");
        fs::create_dir(aisw_home.join("config.json.tmp")).unwrap();

        let err = run_in(
            from_live_args(Tool::Gemini, "work"),
            &aisw_home,
            OsString::new(),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("config.json.tmp"),
            "unexpected error: {err:#}"
        );
        assert_eq!(
            String::from_utf8(ps.read_file(Tool::Gemini, "work", ".env").unwrap()).unwrap(),
            "GEMINI_API_KEY=old-key\n"
        );
        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Gemini)["work"].label.as_deref(),
            Some("old label")
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

    #[test]
    fn from_live_antigravity_creates_profile_and_activates() {
        with_env_lock(|| {
            let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let tmp = tempdir().unwrap();
            let aisw_home = tmp.path().join("aisw");
            let user_home = tmp.path().join("user");
            let keyring_dir = tmp.path().join("keyring");
            fs::create_dir_all(&aisw_home).unwrap();
            fs::create_dir_all(&user_home).unwrap();
            let _home = EnvVarGuard::set("HOME", user_home.to_str().unwrap());
            write_antigravity_live_state(&user_home, &keyring_dir, "{\"session\":\"live-secret\"}");

            run_in(
                from_live_args(Tool::Antigravity, "work"),
                &aisw_home,
                OsString::new(),
            )
            .unwrap();

            let ps = ProfileStore::new(&aisw_home);
            assert!(ps.exists(Tool::Antigravity, "work"));
            assert_eq!(
                String::from_utf8(
                    ps.read_file(Tool::Antigravity, "work", "keyring-secret.json")
                        .unwrap()
                )
                .unwrap(),
                "{\"session\":\"live-secret\"}"
            );
            assert!(ps
                .profile_dir(Tool::Antigravity, "work")
                .join("app/settings.json")
                .exists());
            assert!(ps
                .profile_dir(Tool::Antigravity, "work")
                .join("shared/hooks.json")
                .exists());

            let config = ConfigStore::new(&aisw_home).load().unwrap();
            assert_eq!(config.active_for(Tool::Antigravity), Some("work"));
        });
    }
}
