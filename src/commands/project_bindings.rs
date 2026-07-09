use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::cli::{ProjectBindingsArgs, ProjectBindingsCommand, ProjectBindingsListArgs};
use crate::machine;
use crate::output;
use crate::workspace::{
    detect_repo, load_repo_local_config, repo_local_config_path, WorkspaceStore,
};

#[derive(Debug, Serialize)]
struct ProjectBindingsResult {
    cwd: String,
    repo_local_binding: Option<RepoLocalBindingResult>,
    user_bindings: UserBindingsResult,
}

#[derive(Debug, Serialize)]
struct RepoLocalBindingResult {
    repo_root: String,
    config_path: String,
    context: String,
}

#[derive(Debug, Serialize)]
struct UserBindingsResult {
    default_context: Option<String>,
    path_rules: Vec<PathRuleResult>,
    git_remote_rules: Vec<GitRemoteRuleResult>,
}

#[derive(Debug, Serialize)]
struct PathRuleResult {
    path: String,
    context: String,
}

#[derive(Debug, Serialize)]
struct GitRemoteRuleResult {
    pattern: String,
    context: String,
}

pub fn run(args: ProjectBindingsArgs, home: &Path) -> Result<()> {
    match args.command {
        ProjectBindingsCommand::List(args) => list(args, home),
    }
}

fn list(args: ProjectBindingsListArgs, home: &Path) -> Result<()> {
    let cwd = std::env::current_dir().context("could not determine current directory")?;
    let repo = detect_repo(&cwd)?;
    let workspace_config = WorkspaceStore::new(home).load()?;

    let repo_local_binding = if let Some(repo) = repo.as_ref() {
        load_repo_local_config(repo)?.map(|local| RepoLocalBindingResult {
            repo_root: repo.root.display().to_string(),
            config_path: repo_local_config_path(repo).display().to_string(),
            context: local.context,
        })
    } else {
        None
    };

    let mut path_rules = workspace_config
        .path_rules
        .iter()
        .map(|rule| PathRuleResult {
            path: rule.path.clone(),
            context: rule.context.clone(),
        })
        .collect::<Vec<_>>();
    path_rules.sort_by(|a, b| a.path.cmp(&b.path));

    let mut git_remote_rules = workspace_config
        .git_remote_rules
        .iter()
        .map(|rule| GitRemoteRuleResult {
            pattern: rule.pattern.clone(),
            context: rule.context.clone(),
        })
        .collect::<Vec<_>>();
    git_remote_rules.sort_by(|a, b| a.pattern.cmp(&b.pattern));

    let result = ProjectBindingsResult {
        cwd: cwd.display().to_string(),
        repo_local_binding,
        user_bindings: UserBindingsResult {
            default_context: workspace_config.default_context,
            path_rules,
            git_remote_rules,
        },
    };

    if args.json {
        machine::print_success("project_bindings_list", result)?;
        return Ok(());
    }

    output::print_title("Project bindings");
    output::print_kv("Cwd", &result.cwd);
    if let Some(repo_local) = result.repo_local_binding.as_ref() {
        output::print_blank_line();
        output::print_kv("Repo root", &repo_local.repo_root);
        output::print_kv("Repo config", &repo_local.config_path);
        output::print_kv("Repo context", &repo_local.context);
    }

    output::print_blank_line();
    output::print_kv(
        "Default context",
        result
            .user_bindings
            .default_context
            .as_deref()
            .unwrap_or("none"),
    );

    if result.user_bindings.path_rules.is_empty()
        && result.user_bindings.git_remote_rules.is_empty()
    {
        output::print_blank_line();
        output::print_empty_state("No user workspace bindings saved.");
        return Ok(());
    }

    for rule in &result.user_bindings.path_rules {
        output::print_kv(&format!("Path {}", rule.path), &rule.context);
    }
    for rule in &result.user_bindings.git_remote_rules {
        output::print_kv(&format!("Remote {}", rule.pattern), &rule.context);
    }

    Ok(())
}
