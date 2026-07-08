use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::cli::{RepairArgs, RepairFix};
use crate::config::ConfigStore;
use crate::machine;
use crate::runtime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RepairMode {
    DryRun,
    Apply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RepairStatus {
    Pass,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ActionStatus {
    Planned,
    Applied,
}

#[derive(Debug, Clone, Serialize)]
struct RepairSummary {
    status: RepairStatus,
    issues_found: usize,
    actions_planned: usize,
    actions_applied: usize,
    issues_remaining: usize,
}

#[derive(Debug, Clone, Serialize)]
struct RepairAction {
    fix: &'static str,
    kind: &'static str,
    path: String,
    status: ActionStatus,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct RepairResult {
    mode: RepairMode,
    requested_fixes: Vec<&'static str>,
    summary: RepairSummary,
    actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionKind {
    CreateDir,
    CreateConfig,
    SetDirPermissions,
    SetFilePermissions,
}

#[derive(Debug, Clone)]
struct PlannedAction {
    fix: RepairFix,
    kind: ActionKind,
    path: PathBuf,
    detail: String,
}

pub fn run(args: RepairArgs, home: &Path) -> Result<()> {
    let requested_fixes = normalized_fixes(&args.fix);
    let mode = if args.apply {
        RepairMode::Apply
    } else {
        RepairMode::DryRun
    };

    let actions = plan_actions(home, &requested_fixes)?;
    let result = if mode == RepairMode::Apply {
        apply_actions(home, &requested_fixes, actions)?
    } else {
        dry_run_result(&requested_fixes, actions)
    };

    if !runtime::is_quiet() {
        if args.json {
            machine::print_success("repair", result)?;
        } else {
            print_text(&result);
        }
    }

    Ok(())
}

fn normalized_fixes(explicit: &[RepairFix]) -> Vec<RepairFix> {
    if explicit.is_empty() {
        vec![RepairFix::Home, RepairFix::Permissions]
    } else {
        explicit.to_vec()
    }
}

fn plan_actions(home: &Path, requested_fixes: &[RepairFix]) -> Result<Vec<PlannedAction>> {
    let mut actions = Vec::new();

    if requested_fixes.contains(&RepairFix::Home) {
        if !home.exists() {
            actions.push(PlannedAction {
                fix: RepairFix::Home,
                kind: ActionKind::CreateDir,
                path: home.to_path_buf(),
                detail: "create AISW_HOME directory".to_owned(),
            });
        }

        let config_path = home.join("config.json");
        if !config_path.exists() {
            actions.push(PlannedAction {
                fix: RepairFix::Home,
                kind: ActionKind::CreateConfig,
                path: config_path,
                detail: "create default config.json".to_owned(),
            });
        }
    }

    if requested_fixes.contains(&RepairFix::Permissions) && home.exists() {
        collect_permission_repairs(home, &mut actions)?;
    }

    Ok(actions)
}

fn dry_run_result(requested_fixes: &[RepairFix], actions: Vec<PlannedAction>) -> RepairResult {
    let issues_found = actions.len();
    let status = if issues_found == 0 {
        RepairStatus::Pass
    } else {
        RepairStatus::Warn
    };

    RepairResult {
        mode: RepairMode::DryRun,
        requested_fixes: requested_fixes
            .iter()
            .map(|fix| repair_fix_name(*fix))
            .collect(),
        summary: RepairSummary {
            status,
            issues_found,
            actions_planned: issues_found,
            actions_applied: 0,
            issues_remaining: issues_found,
        },
        actions: actions
            .into_iter()
            .map(|action| RepairAction {
                fix: repair_fix_name(action.fix),
                kind: action_kind_name(action.kind),
                path: action.path.display().to_string(),
                status: ActionStatus::Planned,
                detail: action.detail,
            })
            .collect(),
    }
}

fn apply_actions(
    home: &Path,
    requested_fixes: &[RepairFix],
    actions: Vec<PlannedAction>,
) -> Result<RepairResult> {
    for action in &actions {
        apply_one(home, action)?;
    }

    let issues_remaining = plan_actions(home, requested_fixes)?.len();
    let issues_found = actions.len();

    Ok(RepairResult {
        mode: RepairMode::Apply,
        requested_fixes: requested_fixes
            .iter()
            .map(|fix| repair_fix_name(*fix))
            .collect(),
        summary: RepairSummary {
            status: if issues_remaining == 0 {
                RepairStatus::Pass
            } else {
                RepairStatus::Warn
            },
            issues_found,
            actions_planned: issues_found,
            actions_applied: issues_found,
            issues_remaining,
        },
        actions: actions
            .into_iter()
            .map(|action| RepairAction {
                fix: repair_fix_name(action.fix),
                kind: action_kind_name(action.kind),
                path: action.path.display().to_string(),
                status: ActionStatus::Applied,
                detail: action.detail,
            })
            .collect(),
    })
}

fn apply_one(home: &Path, action: &PlannedAction) -> Result<()> {
    match action.kind {
        ActionKind::CreateDir => {
            fs::create_dir_all(&action.path)
                .with_context(|| format!("could not create {}", action.path.display()))?;
        }
        ActionKind::CreateConfig => {
            fs::create_dir_all(home)
                .with_context(|| format!("could not create {}", home.display()))?;
            ConfigStore::new(home).load()?;
        }
        ActionKind::SetDirPermissions => {
            set_dir_permissions_700(&action.path)?;
        }
        ActionKind::SetFilePermissions => {
            set_file_permissions_600(&action.path)?;
        }
    }
    Ok(())
}

fn collect_permission_repairs(home: &Path, actions: &mut Vec<PlannedAction>) -> Result<()> {
    let root_meta =
        fs::symlink_metadata(home).with_context(|| format!("could not stat {}", home.display()))?;
    if root_meta.is_dir() {
        maybe_collect_dir_permissions(home, actions);
    }
    collect_permission_repairs_recursive(home, actions)
}

fn collect_permission_repairs_recursive(
    path: &Path,
    actions: &mut Vec<PlannedAction>,
) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("could not read {}", path.display()))? {
        let entry = entry.with_context(|| format!("error reading {}", path.display()))?;
        let entry_path = entry.path();
        let meta = fs::symlink_metadata(&entry_path)
            .with_context(|| format!("could not stat {}", entry_path.display()))?;

        if meta.file_type().is_symlink() {
            continue;
        }

        if meta.is_dir() {
            maybe_collect_dir_permissions(&entry_path, actions);
            collect_permission_repairs_recursive(&entry_path, actions)?;
        } else if meta.is_file() {
            maybe_collect_file_permissions(&entry_path, actions);
        }
    }

    Ok(())
}

fn maybe_collect_dir_permissions(path: &Path, actions: &mut Vec<PlannedAction>) {
    #[cfg(unix)]
    {
        let mode = match fs::metadata(path) {
            Ok(meta) => meta.permissions().mode() & 0o777,
            Err(_) => return,
        };
        if mode != 0o700 {
            actions.push(PlannedAction {
                fix: RepairFix::Permissions,
                kind: ActionKind::SetDirPermissions,
                path: path.to_path_buf(),
                detail: format!("normalize directory permissions from {:04o} to 0700", mode),
            });
        }
    }
}

fn maybe_collect_file_permissions(path: &Path, actions: &mut Vec<PlannedAction>) {
    #[cfg(unix)]
    {
        let mode = match fs::metadata(path) {
            Ok(meta) => meta.permissions().mode() & 0o777,
            Err(_) => return,
        };
        if mode != 0o600 {
            actions.push(PlannedAction {
                fix: RepairFix::Permissions,
                kind: ActionKind::SetFilePermissions,
                path: path.to_path_buf(),
                detail: format!("normalize file permissions from {:04o} to 0600", mode),
            });
        }
    }
}

#[cfg(unix)]
fn set_dir_permissions_700(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("could not set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_dir_permissions_700(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions_600(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_file_permissions_600(_path: &Path) -> Result<()> {
    Ok(())
}

fn repair_fix_name(fix: RepairFix) -> &'static str {
    match fix {
        RepairFix::Home => "home",
        RepairFix::Permissions => "permissions",
    }
}

fn action_kind_name(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::CreateDir => "create_dir",
        ActionKind::CreateConfig => "create_config",
        ActionKind::SetDirPermissions => "set_dir_permissions",
        ActionKind::SetFilePermissions => "set_file_permissions",
    }
}

fn print_text(result: &RepairResult) {
    crate::output::print_title("Repair");
    crate::output::print_kv(
        "Mode",
        match result.mode {
            RepairMode::DryRun => "dry_run",
            RepairMode::Apply => "apply",
        },
    );
    crate::output::print_kv(
        "Status",
        match result.summary.status {
            RepairStatus::Pass => "pass",
            RepairStatus::Warn => "warn",
        },
    );
    crate::output::print_kv("Issues found", result.summary.issues_found.to_string());
    crate::output::print_kv(
        "Actions planned",
        result.summary.actions_planned.to_string(),
    );
    crate::output::print_kv(
        "Actions applied",
        result.summary.actions_applied.to_string(),
    );
    crate::output::print_kv(
        "Issues remaining",
        result.summary.issues_remaining.to_string(),
    );
    crate::output::print_blank_line();

    for action in &result.actions {
        let verb = match action.status {
            ActionStatus::Planned => "plan",
            ActionStatus::Applied => "applied",
        };
        crate::output::print_info(format!(
            "[{}] {} {} ({})",
            verb, action.kind, action.path, action.fix
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn plan_actions_reports_missing_home_and_config() {
        let dir = tempdir().unwrap();
        let home = dir.path().join("missing-home");

        let actions = plan_actions(&home, &[RepairFix::Home, RepairFix::Permissions]).unwrap();

        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0].kind, ActionKind::CreateDir));
        assert!(matches!(actions[1].kind, ActionKind::CreateConfig));
    }

    #[cfg(unix)]
    #[test]
    fn plan_actions_reports_broad_permissions() {
        let dir = tempdir().unwrap();
        let home = dir.path().join("aisw");
        fs::create_dir_all(home.join("profiles").join("claude").join("work")).unwrap();
        fs::write(home.join("config.json"), b"{}").unwrap();
        fs::write(
            home.join("profiles")
                .join("claude")
                .join("work")
                .join("credentials.json"),
            b"{}",
        )
        .unwrap();
        fs::set_permissions(&home, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(home.join("config.json"), fs::Permissions::from_mode(0o644)).unwrap();

        let actions = plan_actions(&home, &[RepairFix::Permissions]).unwrap();

        assert!(actions
            .iter()
            .any(|action| matches!(action.kind, ActionKind::SetDirPermissions)));
        assert!(actions
            .iter()
            .any(|action| matches!(action.kind, ActionKind::SetFilePermissions)));
    }
}
