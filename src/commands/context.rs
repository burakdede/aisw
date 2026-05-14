use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Result};
use chrono::Utc;

use crate::cli::{
    ContextArgs, ContextCommand, ContextCreateArgs, ContextListArgs, ContextRemoveArgs,
    ContextRenameArgs, ContextSetArgs, ContextUnsetArgs, ContextUseArgs,
};
use crate::config::{ConfigStore, ContextEntry, ContextProfiles};
use crate::output;
use crate::profile::validate_profile_name;
use crate::types::Tool;

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
    let profiles = profile_map_from_options(args.claude, args.codex, args.gemini);
    if profiles.is_empty() {
        bail!(
            "context create requires at least one tool mapping.\n  \
             Re-run with one or more of: --claude <profile>, --codex <profile>, --gemini <profile>"
        );
    }

    let store = ConfigStore::new(home);
    let config = store.load()?;
    ensure_profiles_exist(&config, &profiles)?;

    let mut context_profiles = ContextProfiles::default();
    for (tool, profile) in profiles {
        context_profiles.insert(tool, profile);
    }

    store.create_context(&args.context_name, ContextEntry::new(context_profiles, Utc::now()))?;

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
                    || entry
                        .profiles
                        .iter()
                        .any(|(tool, profile)| {
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
            output::print_kv(
                tool.binary_name(),
                entry.profiles.get(tool).unwrap_or("-"),
            );
        }
    }

    Ok(())
}

fn set(args: ContextSetArgs, home: &Path) -> Result<()> {
    let store = ConfigStore::new(home);
    let config = store.load()?;
    let existing = config
        .context(&args.context_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("context '{}' not found.", args.context_name))?;

    let updates = profile_map_from_options(args.claude, args.codex, args.gemini);
    if updates.is_empty() {
        bail!(
            "context set requires at least one mapping.\n  \
             Re-run with one or more of: --claude <profile>, --codex <profile>, --gemini <profile>"
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
    store.upsert_context(&args.context_name, updated)?;

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
    let existing = config
        .context(&args.context_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("context '{}' not found.", args.context_name))?;

    let mut profiles = existing.profiles.clone();
    let mut changed = false;
    for (tool, selected) in [
        (Tool::Claude, args.claude),
        (Tool::Codex, args.codex),
        (Tool::Gemini, args.gemini),
    ] {
        if selected {
            changed = true;
            profiles.remove(tool);
        }
    }
    if !changed {
        bail!(
            "context unset requires at least one tool flag.\n  \
             Re-run with one or more of: --claude, --codex, --gemini"
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
    store.upsert_context(&args.context_name, updated)?;

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
    ConfigStore::new(home).remove_context(&args.context_name)?;

    output::print_title("Removed context");
    output::print_kv("Context", &args.context_name);
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Saved context deleted.");
    Ok(())
}

fn rename(args: ContextRenameArgs, home: &Path) -> Result<()> {
    validate_profile_name(&args.new_name)?;
    ConfigStore::new(home).rename_context(&args.old_name, &args.new_name)?;

    output::print_title("Renamed context");
    output::print_kv("Previous", &args.old_name);
    output::print_kv("New", &args.new_name);
    output::print_blank_line();
    output::print_effects_header();
    output::print_effect("Saved context renamed.");
    Ok(())
}

fn use_context(_args: ContextUseArgs, _home: &Path) -> Result<()> {
    bail!("context use is not implemented yet")
}

fn profile_map_from_options(
    claude: Option<String>,
    codex: Option<String>,
    gemini: Option<String>,
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
    profiles
}

fn ensure_profiles_exist(config: &crate::config::Config, profiles: &HashMap<Tool, String>) -> Result<()> {
    for (tool, profile) in profiles {
        if !config.profiles_for(*tool).contains_key(profile) {
            bail!("profile '{}' not found for {}.", profile, tool);
        }
    }
    Ok(())
}

fn normalized_profiles_json(profiles: &ContextProfiles) -> serde_json::Value {
    serde_json::json!({
        "claude": profiles.get(Tool::Claude),
        "codex": profiles.get(Tool::Codex),
        "gemini": profiles.get(Tool::Gemini),
    })
}
