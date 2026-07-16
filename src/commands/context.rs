use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;

use crate::auth;
use crate::cli::{
    ContextArgs, ContextCommand, ContextCreateArgs, ContextListArgs, ContextRemoveArgs,
    ContextRenameArgs, ContextSetArgs, ContextUnsetArgs, ContextUseArgs,
};
use crate::commands::use_::{
    active_map, apply_resolved_profile_switch, backup_ids_for, diff_backup_ids, live_match_map,
    state_mode_map, ResolvedProfileSwitch,
};
use crate::config::{Config, ConfigStore, ContextEntry, ContextProfiles};
use crate::error::AiswError;
use crate::live_apply::LiveFileChange;
use crate::machine;
use crate::output;
use crate::profile::validate_profile_name;
use crate::types::{StateMode, Tool};

pub fn run(args: ContextArgs, home: &Path) -> Result<()> {
    match args.command {
        ContextCommand::Create(args) => create(args, home),
        ContextCommand::List(args) => list(args, home),
        ContextCommand::Use(args) => use_context(args, home),
        ContextCommand::Set(args) => set(args, home),
        ContextCommand::Unset(args) => unset(args, home),
        ContextCommand::Remove(args) => remove(args, home),
        ContextCommand::Rename(args) => rename(args, home),
    }
}

fn create(args: ContextCreateArgs, home: &Path) -> Result<()> {
    validate_profile_name(&args.context_name)?;
    let profiles = profile_map_from_options(args.claude, args.codex, args.gemini, args.antigravity);
    if profiles.is_empty() {
        bail!(
            "context create requires at least one tool mapping.\n  \
             Re-run with one or more of: --claude <profile>, --codex <profile>, --gemini <profile>, --antigravity <profile>"
        );
    }

    let store = ConfigStore::new(home);
    let config = store.load()?;
    ensure_profiles_exist(&config, &profiles)?;

    let mut context_profiles = ContextProfiles::default();
    for (tool, profile) in profiles {
        context_profiles.insert(tool, profile);
    }

    let config = store.create_context(
        &args.context_name,
        ContextEntry::new(context_profiles, Utc::now()),
    )?;

    if args.json {
        let entry = config
            .context(&args.context_name)
            .context("created context missing from config")?;
        machine::print_success(
            "context_create",
            serde_json::json!({
                "context": context_json(&args.context_name, entry),
                "context_count": config.contexts().len(),
            }),
        )?;
        return Ok(());
    }

    output::print_title("Created context");
    output::print_kv("Context", &args.context_name);
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Saved tool-to-profile mappings.");
    output::print_blank_line();
    output::print_next_step("Run 'aisw context list' to review saved contexts.");
    Ok(())
}

fn list(args: ContextListArgs, home: &Path) -> Result<()> {
    let config = ConfigStore::new(home).load()?;
    let mut contexts: Vec<_> = config.contexts().iter().collect();
    contexts.sort_by(|a, b| a.0.cmp(b.0));

    if let Some(search) = args.search.as_deref() {
        let needle = search.trim().to_ascii_lowercase();
        if !needle.is_empty() {
            contexts.retain(|(name, entry)| {
                name.to_ascii_lowercase().contains(&needle)
                    || entry.profiles.iter().any(|(tool, profile)| {
                        tool.binary_name().to_ascii_lowercase().contains(&needle)
                            || profile.to_ascii_lowercase().contains(&needle)
                    })
            });
        }
    }

    if args.json {
        let json = serde_json::json!({
            "contexts": contexts.iter().map(|(name, entry)| {
                serde_json::json!({
                    "name": name,
                    "profiles": normalized_profiles_json(&entry.profiles),
                    "created_at": entry.created_at,
                    "updated_at": entry.updated_at,
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    output::print_title("Contexts");
    if contexts.is_empty() {
        output::print_empty_state("No contexts found.");
        output::print_blank_line();
        output::print_next_step("Run 'aisw context create <name> --claude <profile>' to add one.");
        return Ok(());
    }

    for (idx, (name, entry)) in contexts.iter().enumerate() {
        if idx > 0 {
            output::print_blank_line();
        }
        output::print_kv("Name", name);
        for tool in Tool::ALL {
            output::print_kv(tool.binary_name(), entry.profiles.get(tool).unwrap_or("-"));
        }
    }

    Ok(())
}

fn set(args: ContextSetArgs, home: &Path) -> Result<()> {
    let store = ConfigStore::new(home);
    let config = store.load()?;
    let existing =
        config
            .context(&args.context_name)
            .cloned()
            .ok_or_else(|| AiswError::ContextNotFound {
                name: args.context_name.clone(),
            })?;

    let updates = profile_map_from_options(args.claude, args.codex, args.gemini, args.antigravity);
    if updates.is_empty() {
        bail!(
            "context set requires at least one mapping.\n  \
             Re-run with one or more of: --claude <profile>, --codex <profile>, --gemini <profile>, --antigravity <profile>"
        );
    }
    ensure_profiles_exist(&config, &updates)?;

    let mut profiles = existing.profiles.clone();
    for (tool, profile) in updates {
        profiles.insert(tool, profile);
    }

    let updated = ContextEntry {
        profiles,
        created_at: existing.created_at,
        updated_at: Utc::now(),
    };
    let config = store.upsert_context(&args.context_name, updated)?;

    if args.json {
        let entry = config
            .context(&args.context_name)
            .context("updated context missing from config")?;
        machine::print_success(
            "context_set",
            serde_json::json!({
                "context": context_json(&args.context_name, entry),
                "context_count": config.contexts().len(),
            }),
        )?;
        return Ok(());
    }

    output::print_title("Updated context");
    output::print_kv("Context", &args.context_name);
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Context mappings updated.");
    Ok(())
}

fn unset(args: ContextUnsetArgs, home: &Path) -> Result<()> {
    let store = ConfigStore::new(home);
    let config = store.load()?;
    let existing =
        config
            .context(&args.context_name)
            .cloned()
            .ok_or_else(|| AiswError::ContextNotFound {
                name: args.context_name.clone(),
            })?;

    let mut profiles = existing.profiles.clone();
    let mut changed = false;
    for (tool, selected) in [
        (Tool::Claude, args.claude),
        (Tool::Codex, args.codex),
        (Tool::Gemini, args.gemini),
        (Tool::Antigravity, args.antigravity),
    ] {
        if selected {
            changed = true;
            profiles.remove(tool);
        }
    }
    if !changed {
        bail!(
            "context unset requires at least one tool flag.\n  \
             Re-run with one or more of: --claude, --codex, --gemini, --antigravity"
        );
    }
    if profiles.is_empty() {
        bail!(
            "context '{}' would become empty.\n  \
             Use 'aisw context remove {} --yes' instead.",
            args.context_name,
            args.context_name
        );
    }

    let updated = ContextEntry {
        profiles,
        created_at: existing.created_at,
        updated_at: Utc::now(),
    };
    let config = store.upsert_context(&args.context_name, updated)?;

    if args.json {
        let entry = config
            .context(&args.context_name)
            .context("updated context missing from config")?;
        machine::print_success(
            "context_unset",
            serde_json::json!({
                "context": context_json(&args.context_name, entry),
                "context_count": config.contexts().len(),
            }),
        )?;
        return Ok(());
    }

    output::print_title("Updated context");
    output::print_kv("Context", &args.context_name);
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Selected tool mappings removed.");
    Ok(())
}

fn remove(args: ContextRemoveArgs, home: &Path) -> Result<()> {
    if !args.yes {
        bail!(
            "context remove requires confirmation.\n  \
             Re-run with --yes."
        );
    }
    let config = ConfigStore::new(home).remove_context(&args.context_name)?;

    if args.json {
        machine::print_success(
            "context_remove",
            serde_json::json!({
                "removed_context": args.context_name,
                "remaining_contexts": remaining_context_names(&config),
            }),
        )?;
        return Ok(());
    }

    output::print_title("Removed context");
    output::print_kv("Context", &args.context_name);
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Saved context deleted.");
    Ok(())
}

fn rename(args: ContextRenameArgs, home: &Path) -> Result<()> {
    validate_profile_name(&args.new_name)?;
    let config = ConfigStore::new(home).rename_context(&args.old_name, &args.new_name)?;

    if args.json {
        let entry = config
            .context(&args.new_name)
            .context("renamed context missing from config")?;
        machine::print_success(
            "context_rename",
            serde_json::json!({
                "old_name": args.old_name,
                "new_name": args.new_name,
                "context": context_json(&args.new_name, entry),
            }),
        )?;
        return Ok(());
    }

    output::print_title("Renamed context");
    output::print_kv("Previous", &args.old_name);
    output::print_kv("New", &args.new_name);
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Saved context renamed.");
    Ok(())
}

fn use_context(_args: ContextUseArgs, _home: &Path) -> Result<()> {
    let args = _args;
    let home = _home;
    let user_home = dirs::home_dir().context("could not determine home directory")?;
    let store = ConfigStore::new(home);
    let config = store.load()?;
    let context = config
        .context(&args.context_name)
        .ok_or_else(|| AiswError::ContextNotFound {
            name: args.context_name.clone(),
        })?;
    let switches = resolve_context_switches(&config, context, args.state_mode, home, &user_home)?;
    if switches.is_empty() {
        bail!("context '{}' has no tool mappings.", args.context_name);
    }

    if args.emit_env {
        for switch in &switches {
            apply_resolved_profile_switch(switch, true, home, &user_home)?;
        }
        let activations = switches
            .iter()
            .map(|switch| {
                (
                    switch.tool,
                    switch.profile_name.clone(),
                    switch
                        .tool
                        .supports_state_mode()
                        .then_some(switch.state_mode),
                )
            })
            .collect::<Vec<_>>();
        store.activate_profiles(&activations)?;
        return Ok(());
    }

    let before_backup_ids = backup_ids_for(home, None)?;
    let snapshots = snapshot_live_state_for_context(&switches, &user_home)?;
    let activations = switches
        .iter()
        .map(|switch| {
            (
                switch.tool,
                switch.profile_name.clone(),
                switch
                    .tool
                    .supports_state_mode()
                    .then_some(switch.state_mode),
            )
        })
        .collect::<Vec<_>>();

    let apply_result = (|| -> Result<()> {
        for switch in &switches {
            apply_resolved_profile_switch(switch, false, home, &user_home)?;
        }
        store.activate_profiles(&activations)?;
        Ok(())
    })();

    if let Err(err) = apply_result {
        restore_live_state_for_context(&snapshots, &user_home)?;
        return Err(err);
    }

    if args.json {
        let after_backup_ids = backup_ids_for(home, None)?;
        machine::print_success(
            "context_use",
            serde_json::json!({
                "context": args.context_name,
                "affected_tools": switches.iter().map(|switch| switch.tool.binary_name()).collect::<Vec<_>>(),
                "active": active_map(home, &Tool::ALL)?,
                "state_mode": state_mode_map(home, &Tool::ALL)?,
                "live_match": live_match_map(home, &user_home, &Tool::ALL)?,
                "backup_ids": diff_backup_ids(&before_backup_ids, &after_backup_ids),
                "warnings": Vec::<String>::new(),
            }),
        )?;
        return Ok(());
    }

    output::print_title("Activated context");
    output::print_kv("Context", &args.context_name);
    output::print_blank_line();
    output::print_effects_header();
    for switch in &switches {
        let effect = if switch.tool.supports_state_mode() {
            format!(
                "{} -> {} ({})",
                switch.tool.display_name(),
                switch.profile_name,
                switch.state_mode.display_name()
            )
        } else {
            format!("{} -> {}", switch.tool.display_name(), switch.profile_name)
        };
        output::print_effect(&effect);
    }
    output::print_blank_line();
    output::print_next_step("Run 'aisw status --context' to verify the active mapping.");
    Ok(())
}

fn profile_map_from_options(
    claude: Option<String>,
    codex: Option<String>,
    gemini: Option<String>,
    antigravity: Option<String>,
) -> HashMap<Tool, String> {
    let mut profiles = HashMap::new();
    if let Some(profile) = claude {
        profiles.insert(Tool::Claude, profile);
    }
    if let Some(profile) = codex {
        profiles.insert(Tool::Codex, profile);
    }
    if let Some(profile) = gemini {
        profiles.insert(Tool::Gemini, profile);
    }
    if let Some(profile) = antigravity {
        profiles.insert(Tool::Antigravity, profile);
    }
    profiles
}

fn ensure_profiles_exist(
    config: &crate::config::Config,
    profiles: &HashMap<Tool, String>,
) -> Result<()> {
    for (tool, profile) in profiles {
        if !config.profiles_for(*tool).contains_key(profile) {
            return Err(AiswError::ProfileNotFound {
                tool: *tool,
                name: profile.clone(),
            }
            .into());
        }
    }
    Ok(())
}

fn resolve_context_switches(
    config: &Config,
    context: &ContextEntry,
    state_mode_override: Option<StateMode>,
    home: &Path,
    user_home: &Path,
) -> Result<Vec<ResolvedProfileSwitch>> {
    let mut switches = Vec::new();
    for tool in Tool::ALL {
        let Some(profile_name) = context.profiles.get(tool) else {
            continue;
        };
        let Some(profile_meta) = config.profiles_for(tool).get(profile_name).cloned() else {
            return Err(anyhow!(
                "context references missing profile '{} / {}'.\n  \
                 Update the context with 'aisw context set' or remove it with 'aisw context remove'.",
                tool,
                profile_name
            ));
        };
        profile_meta.credential_backend.validate_for_tool(tool)?;

        let state_mode = if tool.supports_state_mode() {
            state_mode_override.unwrap_or(StateMode::Isolated)
        } else {
            StateMode::Isolated
        };
        let profile_store = crate::profile::ProfileStore::new(home);
        if tool == Tool::Codex && state_mode == StateMode::Shared {
            let classification = auth::codex::classify_profile(
                &profile_store,
                profile_name,
                profile_meta.auth_method,
                profile_meta.credential_backend,
            )?;
            if classification.is_chatgpt_managed() {
                return Err(AiswError::UnsupportedCodexSharedChatgptAuthSwitch {
                    profile: profile_name.to_owned(),
                    imported_bootstrap: classification.is_imported_bootstrap(),
                }
                .into());
            }
        }
        if tool == Tool::Claude && state_mode == StateMode::Isolated {
            let classification = auth::claude::classify_profile(
                user_home,
                &profile_store,
                profile_name,
                profile_meta.auth_method,
                profile_meta.credential_backend,
            )?;
            if classification.blocks_isolated_mode() {
                return Err(AiswError::UnsupportedClaudeMacosOauthIsolation {
                    profile: profile_name.to_owned(),
                }
                .into());
            }
        }

        switches.push(ResolvedProfileSwitch {
            tool,
            profile_name: profile_name.to_owned(),
            profile_meta,
            state_mode,
            backup_on_switch: config.settings.backup_on_switch,
        });
    }
    Ok(switches)
}

#[derive(Debug, Clone)]
enum LiveStateSnapshot {
    Claude {
        credentials: Option<auth::claude::LiveCredentialSnapshot>,
        oauth_account_metadata: Option<Vec<u8>>,
    },
    Codex {
        files: Vec<FileSnapshot>,
    },
    Gemini {
        dir: std::path::PathBuf,
        files: Vec<NamedFileSnapshot>,
    },
    Antigravity {
        snapshot: auth::antigravity::LiveSnapshot,
    },
}

#[derive(Debug, Clone)]
struct FileSnapshot {
    path: std::path::PathBuf,
    bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct NamedFileSnapshot {
    file_name: OsString,
    bytes: Vec<u8>,
}

fn snapshot_live_state_for_context(
    switches: &[ResolvedProfileSwitch],
    user_home: &Path,
) -> Result<HashMap<Tool, LiveStateSnapshot>> {
    let mut snapshots = HashMap::new();
    for switch in switches {
        let snapshot = match switch.tool {
            Tool::Claude => LiveStateSnapshot::Claude {
                credentials: auth::claude::live_credentials_snapshot_for_import(user_home)?,
                oauth_account_metadata: auth::claude::read_live_oauth_account_metadata_for_import(
                    user_home,
                )?,
            },
            Tool::Codex => LiveStateSnapshot::Codex {
                files: vec![
                    snapshot_file(user_home.join(".codex").join("auth.json"))?,
                    snapshot_file(user_home.join(".codex").join("config.toml"))?,
                ],
            },
            Tool::Gemini => {
                let dir = user_home.join(".gemini");
                LiveStateSnapshot::Gemini {
                    files: snapshot_regular_files(&dir)?,
                    dir,
                }
            }
            Tool::Antigravity => LiveStateSnapshot::Antigravity {
                snapshot: auth::antigravity::capture_live_snapshot(user_home)?,
            },
        };
        snapshots.insert(switch.tool, snapshot);
    }
    Ok(snapshots)
}

fn restore_live_state_for_context(
    snapshots: &HashMap<Tool, LiveStateSnapshot>,
    user_home: &Path,
) -> Result<()> {
    for tool in Tool::ALL.iter().rev() {
        let Some(snapshot) = snapshots.get(tool) else {
            continue;
        };
        match snapshot {
            LiveStateSnapshot::Claude {
                credentials,
                oauth_account_metadata,
            } => auth::claude::restore_live_state_after_oauth_add(
                credentials.clone(),
                oauth_account_metadata.clone(),
                user_home,
            )?,
            LiveStateSnapshot::Codex { files } => restore_file_snapshots(files)?,
            LiveStateSnapshot::Gemini { dir, files } => restore_regular_files(dir, files)?,
            LiveStateSnapshot::Antigravity { snapshot } => {
                auth::antigravity::restore_snapshot_to_live(snapshot, user_home)?
            }
        }
    }
    Ok(())
}

fn snapshot_file(path: std::path::PathBuf) -> Result<FileSnapshot> {
    let bytes = if path.exists() {
        Some(std::fs::read(&path).with_context(|| format!("could not read {}", path.display()))?)
    } else {
        None
    };
    Ok(FileSnapshot { path, bytes })
}

fn snapshot_regular_files(dir: &Path) -> Result<Vec<NamedFileSnapshot>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for file in auth::files::list_regular_files(dir)? {
        files.push(NamedFileSnapshot {
            file_name: file.file_name,
            bytes: std::fs::read(&file.path)
                .with_context(|| format!("could not read {}", file.path.display()))?,
        });
    }
    Ok(files)
}

fn restore_file_snapshots(files: &[FileSnapshot]) -> Result<()> {
    let changes = files
        .iter()
        .map(|snapshot| match &snapshot.bytes {
            Some(bytes) => LiveFileChange::write(snapshot.path.clone(), bytes.clone()),
            None => LiveFileChange::delete(snapshot.path.clone()),
        })
        .collect::<Vec<_>>();
    crate::live_apply::apply_transaction(changes)
}

fn restore_regular_files(dir: &Path, files: &[NamedFileSnapshot]) -> Result<()> {
    let mut changes = Vec::new();
    let expected = files
        .iter()
        .map(|file| file.file_name.clone())
        .collect::<HashSet<_>>();

    if dir.exists() {
        for current in auth::files::list_regular_files(dir)? {
            if !expected.contains(&current.file_name) {
                changes.push(LiveFileChange::delete(current.path));
            }
        }
    }

    for file in files {
        changes.push(LiveFileChange::write(
            dir.join(&file.file_name),
            file.bytes.clone(),
        ));
    }

    crate::live_apply::apply_transaction(changes)
}

fn normalized_profiles_json(profiles: &ContextProfiles) -> serde_json::Value {
    serde_json::json!({
        "claude": profiles.get(Tool::Claude),
        "codex": profiles.get(Tool::Codex),
        "gemini": profiles.get(Tool::Gemini),
        "antigravity": profiles.get(Tool::Antigravity),
    })
}

fn context_json(name: &str, entry: &ContextEntry) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "profiles": normalized_profiles_json(&entry.profiles),
        "created_at": entry.created_at,
        "updated_at": entry.updated_at,
    })
}

fn remaining_context_names(config: &Config) -> Vec<String> {
    let mut names = config.contexts().keys().cloned().collect::<Vec<_>>();
    names.sort();
    names
}
