use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::cli::{
    WorkspaceArgs, WorkspaceBindArgs, WorkspaceCheckArgs, WorkspaceCommand, WorkspaceDoctorArgs,
    WorkspaceGuardArgs, WorkspaceStatusArgs,
};
use crate::commands::project_bindings::snapshot as project_bindings_snapshot;
use crate::config::ConfigStore;
use crate::machine;
use crate::output;
use crate::types::Tool;
use crate::workspace::{
    collect_workspace_status, detect_repo, guard_mode, load_repo_local_config,
    normalize_remote_pattern, repo_local_config_path, save_repo_local_config,
    validate_context_exists, GitRemoteRule, GuardMode, PathRule, WorkspaceConfig, WorkspaceState,
    WorkspaceStore,
};

pub fn run(args: WorkspaceArgs, home: &Path) -> Result<()> {
    match args.command {
        WorkspaceCommand::Bind(args) => bind(args, home),
        WorkspaceCommand::Status(args) => status(args, home),
        WorkspaceCommand::Doctor(args) => doctor(args, home),
        WorkspaceCommand::Guard(args) => guard(args, home),
        WorkspaceCommand::Check(args) => check(args, home),
    }
}

fn bind(args: WorkspaceBindArgs, home: &Path) -> Result<()> {
    let config = ConfigStore::new(home).load()?;
    validate_context_exists(&config, &args.context)?;
    let cwd = std::env::current_dir().context("could not determine current directory")?;

    if args.default {
        let store = WorkspaceStore::new(home);
        let mut workspace_config = store.load()?;
        workspace_config.default_context = Some(args.context.clone());
        store.save(&workspace_config)?;

        if args.json {
            machine::print_success(
                "workspace_bind",
                json!({
                    "binding": {
                        "scope": "default",
                        "context": args.context,
                    },
                    "project_bindings": project_bindings_snapshot(home, &cwd)?,
                }),
            )?;
            return Ok(());
        }

        output::print_title("Updated workspace default");
        output::print_kv("Context", &args.context);
        output::print_effects_header();
        output::print_effect("Default workspace context saved.");
        return Ok(());
    }

    if let Some(pattern) = args.git_remote.as_deref() {
        let store = WorkspaceStore::new(home);
        let mut workspace_config = store.load()?;
        let normalized = normalize_remote_pattern(pattern);
        upsert_git_remote_rule(&mut workspace_config, &normalized, &args.context);
        store.save(&workspace_config)?;

        if args.json {
            machine::print_success(
                "workspace_bind",
                json!({
                    "binding": {
                        "scope": "git_remote",
                        "pattern": normalized,
                        "context": args.context,
                    },
                    "project_bindings": project_bindings_snapshot(home, &cwd)?,
                }),
            )?;
            return Ok(());
        }

        output::print_title("Bound workspace remote");
        output::print_kv("Pattern", &normalized);
        output::print_kv("Context", &args.context);
        output::print_effects_header();
        output::print_effect("Remote workspace rule saved.");
        return Ok(());
    }

    let target = args.path.as_deref().unwrap_or(".");
    let path = resolve_target_path(target)?;
    if let Some(repo) = detect_repo(&path)? {
        save_repo_local_config(&repo, &args.context)?;

        if args.json {
            machine::print_success(
                "workspace_bind",
                json!({
                    "binding": {
                        "scope": "repo_local",
                        "repo_root": repo.root.display().to_string(),
                        "config_path": repo_local_config_path(&repo).display().to_string(),
                        "context": args.context,
                    },
                    "project_bindings": project_bindings_snapshot(home, &cwd)?,
                }),
            )?;
            return Ok(());
        }

        output::print_title("Bound workspace repo");
        output::print_kv("Repo", repo.root.display().to_string());
        output::print_kv(
            "Config",
            repo_local_config_path(&repo).display().to_string(),
        );
        output::print_kv("Context", &args.context);
        output::print_effects_header();
        output::print_effect("Repo-local workspace binding saved.");
        return Ok(());
    }

    let store = WorkspaceStore::new(home);
    let mut workspace_config = store.load()?;
    upsert_path_rule(
        &mut workspace_config,
        &path.display().to_string(),
        &args.context,
    );
    store.save(&workspace_config)?;

    if args.json {
        machine::print_success(
            "workspace_bind",
            json!({
                "binding": {
                    "scope": "path",
                    "path": path.display().to_string(),
                    "context": args.context,
                },
                "project_bindings": project_bindings_snapshot(home, &cwd)?,
            }),
        )?;
        return Ok(());
    }

    output::print_title("Bound workspace path");
    output::print_kv("Path", path.display().to_string());
    output::print_kv("Context", &args.context);
    output::print_effects_header();
    output::print_effect("User workspace path rule saved.");
    Ok(())
}

fn status(args: WorkspaceStatusArgs, home: &Path) -> Result<()> {
    let cwd = std::env::current_dir().context("could not determine current directory")?;
    let status = collect_workspace_status(home, &cwd)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "workspace": status.workspace,
                "repo_root": status.repo_root,
                "matched_rule": status.matched_rule,
                "expected_context": status.expected_context,
                "active_context": status.active_context,
                "active_profiles": {
                    "claude": status.active_profiles.get(&Tool::Claude).cloned().flatten(),
                    "codex": status.active_profiles.get(&Tool::Codex).cloned().flatten(),
                    "gemini": status.active_profiles.get(&Tool::Gemini).cloned().flatten(),
                },
                "status": status.status.as_str(),
                "recommended_command": status.recommended_command,
            }))?
        );
        return Ok(());
    }

    output::print_title("Workspace");
    output::print_kv("Workspace", status.workspace.display().to_string());
    if let Some(repo_root) = status.repo_root.as_ref() {
        output::print_kv("Repo root", repo_root.display().to_string());
    }
    output::print_kv(
        "Expected",
        status.expected_context.as_deref().unwrap_or("none"),
    );
    output::print_kv("Active", active_profiles_summary(&status.active_profiles));
    if let Some(active_context) = status.active_context.as_deref() {
        output::print_kv("Active context", active_context);
    }
    output::print_kv("Status", status.status.as_str());
    if let Some(rule) = status.matched_rule.as_deref() {
        output::print_kv("Matched rule", rule);
    }
    if let Some(command) = status.recommended_command.as_deref() {
        output::print_kv("Action", command);
    }
    Ok(())
}

fn doctor(args: WorkspaceDoctorArgs, home: &Path) -> Result<()> {
    let cwd = std::env::current_dir().context("could not determine current directory")?;
    let store = WorkspaceStore::new(home);
    let config = store.load()?;
    let main_config = ConfigStore::new(home).load()?;
    let repo = detect_repo(&cwd)?;
    let mut checks = Vec::new();

    checks.push(doctor_check(
        "workspace config",
        if store.path().exists() {
            "pass"
        } else {
            "warn"
        },
        if store.path().exists() {
            format!("Loaded {}", store.path().display())
        } else {
            format!("{} not found; using defaults", store.path().display())
        },
    ));

    for rule in &config.path_rules {
        let status = if main_config.context(&rule.context).is_some() {
            "pass"
        } else {
            "fail"
        };
        checks.push(doctor_check(
            &format!("path rule {}", rule.path),
            status,
            format!("context={}", rule.context),
        ));
    }

    for rule in &config.git_remote_rules {
        let status = if main_config.context(&rule.context).is_some() {
            "pass"
        } else {
            "fail"
        };
        checks.push(doctor_check(
            &format!("remote rule {}", rule.pattern),
            status,
            format!("context={}", rule.context),
        ));
    }

    if let Some(default_context) = config.default_context.as_deref() {
        checks.push(doctor_check(
            "default context",
            if main_config.context(default_context).is_some() {
                "pass"
            } else {
                "fail"
            },
            default_context.to_owned(),
        ));
    }

    if let Some(repo) = repo.as_ref() {
        match load_repo_local_config(repo)? {
            Some(local) => checks.push(doctor_check(
                "repo-local config",
                if main_config.context(&local.context).is_some() {
                    "pass"
                } else {
                    "fail"
                },
                format!(
                    "{} -> {}",
                    repo_local_config_path(repo).display(),
                    local.context
                ),
            )),
            None => checks.push(doctor_check(
                "repo-local config",
                "warn",
                "not set for current repo".to_owned(),
            )),
        }
    }

    let workspace_status = collect_workspace_status(home, &cwd)?;
    let status_level = match workspace_status.status {
        WorkspaceState::Match => "pass",
        WorkspaceState::NoExpectedContext => "warn",
        WorkspaceState::InvalidContext => "fail",
        WorkspaceState::Mismatch | WorkspaceState::AmbiguousActive | WorkspaceState::Unmanaged => {
            "warn"
        }
    };
    checks.push(doctor_check(
        "current workspace",
        status_level,
        format!(
            "status={}, expected={}, active={}",
            workspace_status.status.as_str(),
            workspace_status
                .expected_context
                .as_deref()
                .unwrap_or("none"),
            workspace_status.active_context.as_deref().unwrap_or("none")
        ),
    ));

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "checks": checks }))?
        );
        return Ok(());
    }

    output::print_title("Workspace doctor");
    for check in checks {
        output::print_kv(&format!("{} [{}]", check.name, check.status), check.message);
    }
    Ok(())
}

fn guard(args: WorkspaceGuardArgs, home: &Path) -> Result<()> {
    let store = WorkspaceStore::new(home);
    let mut config = store.load()?;
    config.guard_mode = GuardMode::from(args.mode);
    store.save(&config)?;

    if args.json {
        let cwd = std::env::current_dir().context("could not determine current directory")?;
        machine::print_success(
            "workspace_guard",
            json!({
                "guard_mode": config.guard_mode.display_name(),
                "project_bindings": project_bindings_snapshot(home, &cwd)?,
            }),
        )?;
        return Ok(());
    }

    output::print_title("Updated workspace guard");
    output::print_kv("Mode", config.guard_mode.display_name());
    output::print_effects_header();
    output::print_effect("Default workspace guard mode saved.");
    Ok(())
}

fn check(args: WorkspaceCheckArgs, home: &Path) -> Result<()> {
    let mode = guard_mode(home)?;
    let cwd = std::env::current_dir().context("could not determine current directory")?;
    let status = collect_workspace_status(home, &cwd)?;

    if matches!(
        status.status,
        WorkspaceState::Match | WorkspaceState::NoExpectedContext
    ) {
        return Ok(());
    }

    let expected = status.expected_context.as_deref().unwrap_or("none");
    let action = status
        .recommended_command
        .as_deref()
        .unwrap_or("aisw workspace status");
    let active = status
        .active_context
        .clone()
        .unwrap_or_else(|| active_profiles_summary(&status.active_profiles));

    let tool_label = args
        .tool
        .map(|tool| tool.display_name().to_owned())
        .unwrap_or_else(|| "agent".to_owned());

    if args.prompt {
        output::print_warning_stderr(format!(
            "Workspace guard: expected context '{expected}', current state '{active}' ({status}). Run '{action}'.",
            status = status.status.as_str()
        ));
        return Ok(());
    }

    match mode {
        GuardMode::Warn => {
            output::print_warning_stderr(format!(
                "Workspace guard warning: expected context '{expected}', current state '{active}' ({status}). Run '{action}' before launching {tool_label}.",
                status = status.status.as_str()
            ));
            Ok(())
        }
        GuardMode::Strict => {
            bail!(
                "workspace guard refused to launch {}.\n  Expected context: '{}'\n  Current state: '{}'\n  Status: {}\n  Run '{}'.",
                tool_label,
                expected,
                active,
                status.status.as_str(),
                action
            )
        }
    }
}

fn resolve_target_path(raw: &str) -> Result<PathBuf> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()
        .context("could not determine current directory")?
        .join(path))
}

fn upsert_path_rule(config: &mut WorkspaceConfig, path: &str, context: &str) {
    if let Some(existing) = config.path_rules.iter_mut().find(|rule| rule.path == path) {
        existing.context = context.to_owned();
    } else {
        config.path_rules.push(PathRule {
            path: path.to_owned(),
            context: context.to_owned(),
        });
    }
}

fn upsert_git_remote_rule(config: &mut WorkspaceConfig, pattern: &str, context: &str) {
    if let Some(existing) = config
        .git_remote_rules
        .iter_mut()
        .find(|rule| rule.pattern == pattern)
    {
        existing.context = context.to_owned();
    } else {
        config.git_remote_rules.push(GitRemoteRule {
            pattern: pattern.to_owned(),
            context: context.to_owned(),
        });
    }
}

fn active_profiles_summary(
    active_profiles: &std::collections::HashMap<Tool, Option<String>>,
) -> String {
    Tool::ALL
        .iter()
        .map(|tool| {
            format!(
                "{} {}",
                tool.binary_name(),
                active_profiles
                    .get(tool)
                    .and_then(|value| value.as_deref())
                    .unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(serde::Serialize)]
struct DoctorCheck {
    name: String,
    status: &'static str,
    message: String,
}

fn doctor_check(name: &str, status: &'static str, message: String) -> DoctorCheck {
    DoctorCheck {
        name: name.to_owned(),
        status,
        message,
    }
}
