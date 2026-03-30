use std::collections::HashMap;
use std::fs;
use std::io::IsTerminal;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use dialoguer::{theme::ColorfulTheme, Confirm, Input};

use crate::auth;
use crate::config::{AuthMethod, ConfigStore, ProfileMeta};
use crate::output;
use crate::profile::{validate_profile_name, ProfileStore};
use crate::runtime;
use crate::tool_detection::{self, DetectedTool};
use crate::types::Tool;

// Marker written by shell_hook.rs — must match.
pub(crate) const HOOK_MARKER: &str = "# Added by aisw";

pub(crate) fn run_inner(
    aisw_home: &Path,
    user_home: &Path,
    shell_env: Option<&str>,
    confirmed: bool,
) -> Result<()> {
    // Ensure ~/.aisw/ exists with a default config.json.
    fs::create_dir_all(aisw_home)
        .with_context(|| format!("could not create {}", aisw_home.display()))?;
    let config_store = ConfigStore::new(aisw_home);
    config_store.load()?; // creates config.json with defaults if absent
    output::print_title("Initialize aisw");
    output::print_kv("Home", aisw_home.display().to_string());
    output::print_blank_line();

    let detected_tools = detect_supported_tools();

    // Shell hook installation.
    let shell_name = shell_env
        .and_then(|s| Path::new(s).file_name())
        .and_then(|n| n.to_str());
    output::print_section("Shell integration");
    output::print_kv("Shell", shell_name.unwrap_or("unknown"));
    match shell_name {
        Some(s @ ("bash" | "zsh" | "fish")) => {
            install_shell_hook(user_home, s, confirmed)?;
        }
        Some(name) => {
            output::print_warning(format!(
                "Shell not recognized ({}). Install the hook manually: \
                 aisw shell-hook bash >> ~/.bashrc",
                name
            ));
        }
        None => {
            output::print_warning(
                "Could not detect shell. Install the hook manually: \
                 aisw shell-hook bash >> ~/.bashrc",
            );
        }
    }
    output::print_blank_line();

    print_detected_tools(&detected_tools);

    // Credential import.
    import_credentials(aisw_home, user_home, &detected_tools, confirmed)?;

    output::print_title("Setup complete");
    output::print_next_step(output::next_step_after_init());
    Ok(())
}

fn detect_supported_tools() -> HashMap<Tool, Option<DetectedTool>> {
    Tool::ALL
        .into_iter()
        .map(|tool| (tool, tool_detection::detect(tool)))
        .collect()
}

fn print_detected_tools(detected: &HashMap<Tool, Option<DetectedTool>>) {
    output::print_section("Detected tools");
    output::print_info("aisw checked your PATH for supported coding agent CLIs.");
    output::print_blank_line();

    for tool in Tool::ALL {
        output::print_tool_section(tool);
        print_detection_metadata(detected.get(&tool).and_then(|entry| entry.as_ref()));
        output::print_blank_line();
    }
}

pub(crate) fn rc_file(user_home: &Path, shell: &str) -> PathBuf {
    match shell {
        "bash" => {
            if cfg!(target_os = "macos") {
                user_home.join(".bash_profile")
            } else {
                user_home.join(".bashrc")
            }
        }
        "zsh" => user_home.join(".zshrc"),
        "fish" => user_home.join(".config").join("fish").join("config.fish"),
        _ => unreachable!(),
    }
}

fn install_shell_hook(user_home: &Path, shell: &str, confirmed: bool) -> Result<()> {
    let rc = rc_file(user_home, shell);

    if rc.exists() {
        let contents =
            fs::read_to_string(&rc).with_context(|| format!("could not read {}", rc.display()))?;
        if contents.contains(HOOK_MARKER) {
            output::print_info(format!("Shell hook already installed in {}.", rc.display()));
            return Ok(());
        }
    }

    let should_install = confirmed
        || prompt_yes_no(&format!(
            "Shell: {}\nAdd shell integration to {}? [Y/n] ",
            shell,
            rc.display()
        ));

    if !should_install {
        output::print_info("Skipping shell hook installation.");
        return Ok(());
    }

    if let Some(parent) = rc.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    let hook_line = match shell {
        "bash" | "zsh" => format!("\n{}\neval \"$(aisw shell-hook {})\"\n", HOOK_MARKER, shell),
        "fish" => format!("\n{}\naisw shell-hook fish | source\n", HOOK_MARKER),
        _ => unreachable!(),
    };

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc)
        .with_context(|| format!("could not open {}", rc.display()))?;
    file.write_all(hook_line.as_bytes())
        .with_context(|| format!("could not write to {}", rc.display()))?;

    output::print_info(format!(
        "Appended to {}. Restart your shell or run: source {}",
        rc.display(),
        rc.display()
    ));
    Ok(())
}

fn prompt_yes_no(prompt: &str) -> bool {
    if runtime::is_non_interactive() {
        return false;
    }
    if !std::io::stdin().is_terminal() {
        eprint!("{}", prompt);
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap_or(0);
        return matches!(line.trim(), "" | "y" | "Y");
    }
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt.trim())
        .default(true)
        .interact_opt()
        .unwrap_or(None)
        .unwrap_or(false)
}

fn prompt_line(prompt: &str) -> String {
    if runtime::is_non_interactive() {
        return String::new();
    }
    if !std::io::stdin().is_terminal() {
        eprint!("{}", prompt);
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap_or(0);
        return line.trim().to_owned();
    }
    let (message, default) = parse_prompt(prompt);
    let theme = ColorfulTheme::default();
    let input = Input::<String>::with_theme(&theme).with_prompt(message);
    let input = if let Some(default) = default {
        input.default(default.to_owned())
    } else {
        input
    };
    input.interact_text().unwrap_or_default()
}

fn parse_prompt(prompt: &str) -> (&str, Option<&str>) {
    let trimmed = prompt.trim();
    let Some(start) = trimmed.rfind('[') else {
        return (trimmed, None);
    };
    let Some(end) = trimmed[start..].find(']') else {
        return (trimmed, None);
    };
    let end = start + end;
    let message = trimmed[..start].trim_end_matches(':').trim();
    let default = trimmed[start + 1..end].trim();
    if message.is_empty() || default.is_empty() {
        (trimmed, None)
    } else {
        (message, Some(default))
    }
}

fn import_name_and_label(
    tool: Tool,
    profile_store: &ProfileStore,
    confirmed: bool,
) -> Result<Option<(String, Option<String>)>> {
    if confirmed {
        return Ok(Some(("default".to_owned(), Some("imported".to_owned()))));
    }

    if !prompt_yes_no("  Import these credentials into aisw? [Y/n] ") {
        return Ok(None);
    }

    loop {
        let profile_name = prompt_line("  Profile name [default]: ");
        let profile_name = if profile_name.is_empty() {
            "default".to_owned()
        } else {
            profile_name
        };

        if let Err(err) = validate_profile_name(&profile_name) {
            output::print_warning_stderr(format!("Invalid profile name: {}", err));
            continue;
        }
        if profile_store.exists(tool, &profile_name) {
            output::print_warning_stderr(format!(
                "Profile '{}' already exists for {}. Choose a different name.",
                profile_name, tool
            ));
            continue;
        }

        let label = prompt_line("  Label [imported]: ");
        let label = if label.is_empty() {
            Some("imported".to_owned())
        } else {
            Some(label)
        };

        return Ok(Some((profile_name, label)));
    }
}

fn should_mark_import_active(config_store: &ConfigStore, tool: Tool) -> Result<bool> {
    let config = config_store.load()?;
    Ok(config_store.get_active(&config, tool).is_none())
}

fn activate_imported_profile(
    tool: Tool,
    auth_method: AuthMethod,
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    profile_name: &str,
    user_home: &Path,
) -> Result<()> {
    match tool {
        Tool::Claude => {
            auth::claude::apply_live_credentials(profile_store, profile_name, user_home)?;
        }
        Tool::Codex => {
            auth::codex::apply_live_files(profile_store, profile_name, user_home)?;
        }
        Tool::Gemini => {
            let gemini_dir = user_home.join(".gemini");
            fs::create_dir_all(&gemini_dir)
                .with_context(|| format!("could not create {}", gemini_dir.display()))?;
            match auth_method {
                AuthMethod::ApiKey => {
                    auth::gemini::apply_env_file(
                        profile_store,
                        profile_name,
                        &gemini_dir.join(".env"),
                    )?;
                }
                AuthMethod::OAuth => {
                    auth::gemini::apply_token_cache(profile_store, profile_name, &gemini_dir)?;
                }
            }
        }
    }

    config_store.set_active(tool, profile_name)?;
    Ok(())
}

fn import_credentials(
    aisw_home: &Path,
    user_home: &Path,
    detected: &HashMap<Tool, Option<DetectedTool>>,
    confirmed: bool,
) -> Result<()> {
    output::print_section("Credential onboarding");
    output::print_info(
        "Each tool is checked for existing live credentials that can be imported into aisw.",
    );
    output::print_blank_line();
    import_claude(
        aisw_home,
        user_home,
        detected.get(&Tool::Claude).and_then(|entry| entry.as_ref()),
        confirmed,
    )?;
    import_codex(
        aisw_home,
        user_home,
        detected.get(&Tool::Codex).and_then(|entry| entry.as_ref()),
        confirmed,
    )?;
    import_gemini(
        aisw_home,
        user_home,
        detected.get(&Tool::Gemini).and_then(|entry| entry.as_ref()),
        confirmed,
    )?;
    Ok(())
}

fn print_detection_metadata(detected: Option<&DetectedTool>) {
    match detected {
        Some(tool) => {
            output::print_kv("Status", "detected");
            if let Some(version) = tool.version.as_deref() {
                output::print_kv("Version", version);
            }
            output::print_kv("Path", tool.binary_path.display().to_string());
        }
        None => {
            output::print_kv("Status", "not detected");
        }
    }
}

fn print_import_header(tool: Tool, detected: Option<&DetectedTool>) {
    output::print_tool_section(tool);
    output::print_kv("Detected", if detected.is_some() { "yes" } else { "no" });
}

fn extract_json_string_field(bytes: &[u8], field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    value.get(field)?.as_str().map(ToOwned::to_owned)
}

fn extract_gemini_api_key(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("GEMINI_API_KEY=").map(ToOwned::to_owned))
}

fn import_claude(
    aisw_home: &Path,
    user_home: &Path,
    detected: Option<&DetectedTool>,
    confirmed: bool,
) -> Result<()> {
    print_import_header(Tool::Claude, detected);
    let local_state = auth::claude::live_local_state_dir(user_home);
    output::print_kv(
        "Local state",
        local_state
            .as_ref()
            .map(|path| format!("found {}", path.display()))
            .unwrap_or_else(|| "not found".to_owned()),
    );

    let Some(snapshot) = auth::claude::live_credentials_snapshot_for_import(user_home)? else {
        if auth::claude::keychain_import_supported() && local_state.is_some() {
            output::print_kv("Credentials", "not found in file or Keychain");
            output::print_info(
                "Claude local state exists, but aisw could not find importable auth in \
                 ~/.claude/.credentials.json or macOS Keychain.",
            );
        } else {
            output::print_kv("Credentials", "not found");
        }
        output::print_blank_line();
        return Ok(());
    };

    let profile_store = ProfileStore::new(aisw_home);
    let config_store = ConfigStore::new(aisw_home);
    let mark_active = should_mark_import_active(&config_store, Tool::Claude)?;

    let (source_desc, source_bytes) = match snapshot.source {
        auth::claude::LiveCredentialSource::File(path) => {
            (format!("found {}", path.display()), snapshot.bytes)
        }
        auth::claude::LiveCredentialSource::Keychain => {
            ("found macOS Keychain".to_owned(), snapshot.bytes)
        }
    };
    let imported_method = if extract_json_string_field(&source_bytes, "apiKey").is_some() {
        AuthMethod::ApiKey
    } else {
        AuthMethod::OAuth
    };
    if let Some(api_key) = extract_json_string_field(&source_bytes, "apiKey") {
        if let Some(existing_name) = auth::identity::existing_api_key_profile_for_secret(
            &profile_store,
            &config_store,
            Tool::Claude,
            &api_key,
        )? {
            output::print_kv("Credentials", &source_desc);
            output::print_kv("Auth", "api_key");
            output::print_kv("Import", "already managed");
            output::print_info(format!(
                "Live credentials already match profile '{}'.",
                existing_name
            ));
            output::print_blank_line();
            return Ok(());
        }
    }
    if let Some(existing_name) = auth::identity::existing_oauth_profile_for_json_bytes(
        &profile_store,
        &config_store,
        Tool::Claude,
        &source_bytes,
    )? {
        output::print_kv("Credentials", &source_desc);
        output::print_kv("Auth", "oauth");
        output::print_kv("Import", "already managed");
        output::print_info(format!(
            "Live credentials already match profile '{}'.",
            existing_name
        ));
        output::print_blank_line();
        return Ok(());
    }

    if confirmed && profile_store.exists(Tool::Claude, "default") {
        output::print_kv("Credentials", &source_desc);
        output::print_kv("Import", "skipped");
        output::print_info("Profile 'default' already exists.");
        output::print_blank_line();
        return Ok(());
    }

    output::print_kv("Credentials", &source_desc);
    output::print_kv(
        "Auth",
        if imported_method == AuthMethod::ApiKey {
            "api_key"
        } else {
            "oauth"
        },
    );
    let Some((profile_name, label)) =
        import_name_and_label(Tool::Claude, &profile_store, confirmed)?
    else {
        output::print_kv("Import", "skipped");
        output::print_blank_line();
        return Ok(());
    };

    profile_store.create(Tool::Claude, &profile_name)?;
    profile_store.write_file(
        Tool::Claude,
        &profile_name,
        ".credentials.json",
        &source_bytes,
    )?;
    if imported_method == AuthMethod::OAuth {
        auth::identity::ensure_unique_oauth_identity(
            &profile_store,
            &config_store,
            Tool::Claude,
            &profile_name,
        )
        .inspect_err(|_| {
            let _ = profile_store.delete(Tool::Claude, &profile_name);
        })?;
    }
    config_store.add_profile(
        Tool::Claude,
        &profile_name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: imported_method,
            label,
        },
    )?;
    if mark_active {
        activate_imported_profile(
            Tool::Claude,
            imported_method,
            &profile_store,
            &config_store,
            &profile_name,
            user_home,
        )?;
        output::print_success(format!(
            "Imported Claude Code credentials as profile '{}' and marked it active.",
            profile_name
        ));
        output::print_kv("Import", format!("profile '{}'", profile_name));
        output::print_kv("Activation", "active");
    } else {
        output::print_success(format!(
            "Imported Claude Code credentials as profile '{}'.",
            profile_name
        ));
        output::print_kv("Import", format!("profile '{}'", profile_name));
        output::print_kv("Activation", "stored");
    }
    output::print_blank_line();
    Ok(())
}

fn import_codex(
    aisw_home: &Path,
    user_home: &Path,
    detected: Option<&DetectedTool>,
    confirmed: bool,
) -> Result<()> {
    print_import_header(Tool::Codex, detected);
    let local_state = auth::codex::live_local_state_dir(user_home);
    output::print_kv(
        "Local state",
        local_state
            .as_ref()
            .map(|path| format!("found {}", path.display()))
            .unwrap_or_else(|| "not found".to_owned()),
    );

    let Some(snapshot) = auth::codex::live_credentials_snapshot_for_import(user_home)? else {
        if local_state.is_some() {
            let storage = auth::codex::live_auth_storage(user_home)?
                .unwrap_or(auth::codex::LiveAuthStorage::Auto);
            output::print_kv("Credentials", "not found in auth.json");
            match storage {
                auth::codex::LiveAuthStorage::Keyring => output::print_info(
                    "Codex local state exists, but aisw could not find importable auth in \
                     ~/.codex/auth.json. This install appears to use keyring-backed auth, \
                     which init does not import yet.",
                ),
                auth::codex::LiveAuthStorage::Auto => output::print_info(
                    "Codex local state exists, but aisw could not find importable auth in \
                     ~/.codex/auth.json. Codex defaults to its auto auth-storage mode here, \
                     which may be using the OS credential store instead of a file backend.",
                ),
                auth::codex::LiveAuthStorage::File => output::print_info(
                    "Codex local state exists, but aisw could not find importable auth in \
                     ~/.codex/auth.json even though the configured backend is file.",
                ),
                auth::codex::LiveAuthStorage::Unknown => output::print_info(
                    "Codex local state exists, but aisw could not find importable auth in \
                     ~/.codex/auth.json. The configured auth backend is not recognized.",
                ),
            }
            output::print_kv("Auth storage", storage.description());
        } else {
            output::print_kv("Credentials", "not found");
        }
        output::print_blank_line();
        return Ok(());
    };

    let profile_store = ProfileStore::new(aisw_home);
    let config_store = ConfigStore::new(aisw_home);
    let mark_active = should_mark_import_active(&config_store, Tool::Codex)?;

    let (src_desc, source_bytes) = match snapshot.source {
        auth::codex::LiveCredentialSource::File(path) => {
            let bytes = snapshot.bytes;
            (format!("found {}", path.display()), bytes)
        }
    };
    if let Some(secret) = extract_json_string_field(&source_bytes, "token") {
        if let Some(existing_name) = auth::identity::existing_api_key_profile_for_secret(
            &profile_store,
            &config_store,
            Tool::Codex,
            &secret,
        )? {
            output::print_kv("Credentials", &src_desc);
            output::print_kv("Auth", "api_key");
            output::print_kv("Import", "already managed");
            output::print_info(format!(
                "Live credentials already match profile '{}'.",
                existing_name
            ));
            output::print_blank_line();
            return Ok(());
        }
    }
    if let Some(existing_name) = auth::identity::existing_oauth_profile_for_json_bytes(
        &profile_store,
        &config_store,
        Tool::Codex,
        &source_bytes,
    )? {
        output::print_kv("Credentials", &src_desc);
        output::print_kv("Auth", "oauth");
        output::print_kv("Import", "already managed");
        output::print_info(format!(
            "Live credentials already match profile '{}'.",
            existing_name
        ));
        output::print_blank_line();
        return Ok(());
    }

    if confirmed && profile_store.exists(Tool::Codex, "default") {
        output::print_kv("Credentials", &src_desc);
        output::print_kv("Import", "skipped");
        output::print_info("Profile 'default' already exists.");
        output::print_blank_line();
        return Ok(());
    }

    output::print_kv("Credentials", &src_desc);
    output::print_kv("Auth", "oauth");
    let Some((profile_name, label)) =
        import_name_and_label(Tool::Codex, &profile_store, confirmed)?
    else {
        output::print_kv("Import", "skipped");
        output::print_blank_line();
        return Ok(());
    };

    profile_store.create(Tool::Codex, &profile_name)?;
    auth::codex::write_file_store_config(&profile_store, &profile_name)?;
    profile_store.write_file(Tool::Codex, &profile_name, "auth.json", &source_bytes)?;
    auth::identity::ensure_unique_oauth_identity(
        &profile_store,
        &config_store,
        Tool::Codex,
        &profile_name,
    )
    .inspect_err(|_| {
        let _ = profile_store.delete(Tool::Codex, &profile_name);
    })?;
    config_store.add_profile(
        Tool::Codex,
        &profile_name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            label,
        },
    )?;
    if mark_active {
        activate_imported_profile(
            Tool::Codex,
            AuthMethod::OAuth,
            &profile_store,
            &config_store,
            &profile_name,
            user_home,
        )?;
        output::print_success(format!(
            "Imported Codex CLI credentials as profile '{}' and marked it active.",
            profile_name
        ));
        output::print_kv("Import", format!("profile '{}'", profile_name));
        output::print_kv("Activation", "active");
    } else {
        output::print_success(format!(
            "Imported Codex CLI credentials as profile '{}'.",
            profile_name
        ));
        output::print_kv("Import", format!("profile '{}'", profile_name));
        output::print_kv("Activation", "stored");
    }
    output::print_blank_line();
    Ok(())
}

fn import_gemini(
    aisw_home: &Path,
    user_home: &Path,
    detected: Option<&DetectedTool>,
    confirmed: bool,
) -> Result<()> {
    print_import_header(Tool::Gemini, detected);
    let gemini_dir = auth::gemini::live_dir(user_home);
    let env_file = gemini_dir.join(".env");
    let oauth_files = auth::gemini::live_oauth_files_for_import(user_home)?;

    let (src_desc, method) = if env_file.exists() {
        (format!("found {}", env_file.display()), AuthMethod::ApiKey)
    } else if let Some(primary_file) = auth::gemini::preferred_live_oauth_file(&oauth_files) {
        (
            auth::gemini::live_import_source_description(&primary_file.path, oauth_files.len()),
            AuthMethod::OAuth,
        )
    } else {
        output::print_kv("Credentials", "not found");
        output::print_blank_line();
        return Ok(());
    };

    let profile_store = ProfileStore::new(aisw_home);
    let config_store = ConfigStore::new(aisw_home);
    let mark_active = should_mark_import_active(&config_store, Tool::Gemini)?;

    if method == AuthMethod::ApiKey {
        let source_bytes = fs::read(&env_file)
            .with_context(|| format!("could not read {}", env_file.display()))?;
        if let Some(api_key) = extract_gemini_api_key(&source_bytes) {
            if let Some(existing_name) = auth::identity::existing_api_key_profile_for_secret(
                &profile_store,
                &config_store,
                Tool::Gemini,
                &api_key,
            )? {
                output::print_kv("Credentials", &src_desc);
                output::print_kv("Auth", "api_key");
                output::print_kv("Import", "already managed");
                output::print_info(format!(
                    "Live credentials already match profile '{}'.",
                    existing_name
                ));
                output::print_blank_line();
                return Ok(());
            }
        }
    }

    if method == AuthMethod::OAuth {
        if let Some(existing_name) = auth::gemini::existing_oauth_profile_for_live_files(
            &profile_store,
            &config_store,
            &oauth_files,
        )? {
            output::print_kv("Credentials", &src_desc);
            output::print_kv("Auth", "oauth");
            output::print_kv("Import", "already managed");
            output::print_info(format!(
                "Live credentials already match profile '{}'.",
                existing_name
            ));
            output::print_blank_line();
            return Ok(());
        }
    }

    if confirmed && profile_store.exists(Tool::Gemini, "default") {
        output::print_kv("Credentials", &src_desc);
        output::print_kv("Import", "skipped");
        output::print_info("Profile 'default' already exists.");
        output::print_blank_line();
        return Ok(());
    }

    output::print_kv("Credentials", &src_desc);
    output::print_kv(
        "Auth",
        if method == AuthMethod::ApiKey {
            "api_key"
        } else {
            "oauth"
        },
    );
    let Some((profile_name, label)) =
        import_name_and_label(Tool::Gemini, &profile_store, confirmed)?
    else {
        output::print_kv("Import", "skipped");
        output::print_blank_line();
        return Ok(());
    };

    profile_store.create(Tool::Gemini, &profile_name)?;
    if method == AuthMethod::OAuth {
        auth::gemini::copy_live_oauth_files_into_profile(
            &profile_store,
            &profile_name,
            &oauth_files,
        )?;
        auth::identity::ensure_unique_oauth_identity(
            &profile_store,
            &config_store,
            Tool::Gemini,
            &profile_name,
        )
        .inspect_err(|_| {
            let _ = profile_store.delete(Tool::Gemini, &profile_name);
        })?;
    } else {
        profile_store.copy_file_into(Tool::Gemini, &profile_name, &env_file, ".env")?;
    }
    config_store.add_profile(
        Tool::Gemini,
        &profile_name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: method,
            label,
        },
    )?;
    if mark_active {
        activate_imported_profile(
            Tool::Gemini,
            method,
            &profile_store,
            &config_store,
            &profile_name,
            user_home,
        )?;
        output::print_success(format!(
            "Imported Gemini CLI credentials as profile '{}' and marked it active.",
            profile_name
        ));
        output::print_kv("Import", format!("profile '{}'", profile_name));
        output::print_kv("Activation", "active");
    } else {
        output::print_success(format!(
            "Imported Gemini CLI credentials as profile '{}'.",
            profile_name
        ));
        output::print_kv("Import", format!("profile '{}'", profile_name));
        output::print_kv("Activation", "stored");
    }
    output::print_blank_line();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn run(aisw_home: &Path, user_home: &Path, shell: Option<&str>) -> Result<()> {
        run_inner(aisw_home, user_home, shell, true)
    }

    #[test]
    fn creates_aisw_home_and_config() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        run(&aisw_home, &user_home, None).unwrap();

        assert!(aisw_home.join("config.json").exists());
    }

    #[test]
    fn idempotent_does_not_duplicate_hook() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();
        let rc = user_home.join(".zshrc");

        run(&aisw_home, &user_home, Some("/bin/zsh")).unwrap();
        run(&aisw_home, &user_home, Some("/bin/zsh")).unwrap();

        let contents = fs::read_to_string(&rc).unwrap();
        assert_eq!(
            contents.matches("shell-hook").count(),
            1,
            "hook should appear exactly once"
        );
    }

    #[test]
    fn bash_rc_file_appended() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        run(&aisw_home, &user_home, Some("/bin/bash")).unwrap();

        let rc = if cfg!(target_os = "macos") {
            user_home.join(".bash_profile")
        } else {
            user_home.join(".bashrc")
        };
        let contents = fs::read_to_string(&rc).unwrap();
        assert!(contents.contains("shell-hook bash"));
    }

    #[test]
    fn fish_rc_file_appended() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        run(&aisw_home, &user_home, Some("/usr/bin/fish")).unwrap();

        let rc = user_home.join(".config").join("fish").join("config.fish");
        let contents = fs::read_to_string(&rc).unwrap();
        assert!(contents.contains("shell-hook fish | source"));
    }

    #[test]
    fn unknown_shell_does_not_error() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&user_home).unwrap();

        assert!(run(&aisw_home, &user_home, Some("/usr/bin/nushell")).is_ok());
    }

    #[test]
    fn imports_claude_credentials() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        let claude_dir = user_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join(".credentials.json"),
            b"{\"token\":\"oauth\"}",
        )
        .unwrap();

        run(&aisw_home, &user_home, None).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Claude, "default"));
        let contents = ps
            .read_file(Tool::Claude, "default", ".credentials.json")
            .unwrap();
        assert_eq!(contents, b"{\"token\":\"oauth\"}");

        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert!(config.profiles_for(Tool::Claude).contains_key("default"));
        assert_eq!(
            config.profiles_for(Tool::Claude)["default"].auth_method,
            AuthMethod::OAuth
        );
    }

    #[test]
    fn imports_codex_credentials() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        let codex_dir = user_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("auth.json"), b"{\"token\":\"tok\"}").unwrap();

        run(&aisw_home, &user_home, None).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Codex, "default"));
        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Codex)["default"].auth_method,
            AuthMethod::OAuth
        );
    }

    #[test]
    fn imports_gemini_env_as_api_key() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        let gemini_dir = user_home.join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();
        fs::write(gemini_dir.join(".env"), b"GEMINI_API_KEY=abc123\n").unwrap();

        run(&aisw_home, &user_home, None).unwrap();

        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Gemini, "default"));
        let config = ConfigStore::new(&aisw_home).load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Gemini)["default"].auth_method,
            AuthMethod::ApiKey
        );
    }

    #[test]
    fn skip_import_if_default_profile_exists() {
        let tmp = tempdir().unwrap();
        let aisw_home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        let claude_dir = user_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join(".credentials.json"), b"{\"token\":\"v1\"}").unwrap();

        // First run: import succeeds.
        run(&aisw_home, &user_home, None).unwrap();

        // Second run: skip without error.
        run(&aisw_home, &user_home, None).unwrap();

        // Profile still exists, credentials not overwritten.
        let ps = ProfileStore::new(&aisw_home);
        assert!(ps.exists(Tool::Claude, "default"));
    }
}
