use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::commands::status::{collect_status, derive_context_status, DerivedContextStatus};
use crate::config::{Config, ConfigStore};
use crate::error::AiswError;
use crate::types::Tool;

const WORKSPACES_VERSION: u32 = 1;
const WORKSPACES_FILE: &str = "workspaces.json";
const REPO_LOCAL_FILE: &str = "aisw.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceConfig {
    #[serde(default = "workspace_version")]
    pub version: u32,
    #[serde(default)]
    pub guard_mode: GuardMode,
    pub default_context: Option<String>,
    #[serde(default)]
    pub path_rules: Vec<PathRule>,
    #[serde(default)]
    pub git_remote_rules: Vec<GitRemoteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RepoLocalWorkspaceConfig {
    #[serde(default = "workspace_version")]
    pub version: u32,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathRule {
    pub path: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitRemoteRule {
    pub pattern: String,
    pub context: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum GuardMode {
    #[default]
    Warn,
    Strict,
}

impl GuardMode {
    pub fn display_name(self) -> &'static str {
        match self {
            GuardMode::Warn => "warn",
            GuardMode::Strict => "strict",
        }
    }
}

impl From<crate::cli::WorkspaceGuardMode> for GuardMode {
    fn from(mode: crate::cli::WorkspaceGuardMode) -> Self {
        match mode {
            crate::cli::WorkspaceGuardMode::Warn => GuardMode::Warn,
            crate::cli::WorkspaceGuardMode::Strict => GuardMode::Strict,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub remotes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBinding {
    pub workspace: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub expected_context: Option<String>,
    pub matched_rule: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceStatus {
    pub workspace: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub matched_rule: Option<String>,
    pub expected_context: Option<String>,
    pub active_context: Option<String>,
    pub active_profiles: HashMap<Tool, Option<String>>,
    pub status: WorkspaceState,
    pub recommended_command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceState {
    Match,
    Mismatch,
    Unmanaged,
    InvalidContext,
    AmbiguousActive,
    NoExpectedContext,
}

impl WorkspaceState {
    pub fn as_str(self) -> &'static str {
        match self {
            WorkspaceState::Match => "match",
            WorkspaceState::Mismatch => "mismatch",
            WorkspaceState::Unmanaged => "unmanaged",
            WorkspaceState::InvalidContext => "invalid_context",
            WorkspaceState::AmbiguousActive => "ambiguous_active",
            WorkspaceState::NoExpectedContext => "no_expected_context",
        }
    }
}

fn workspace_version() -> u32 {
    WORKSPACES_VERSION
}

pub struct WorkspaceStore {
    path: PathBuf,
}

impl WorkspaceStore {
    pub fn new(home: &Path) -> Self {
        Self {
            path: home.join(WORKSPACES_FILE),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<WorkspaceConfig> {
        if !self.path.exists() {
            return Ok(WorkspaceConfig::default());
        }
        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("could not read {}", self.path.display()))?;
        let mut config: WorkspaceConfig = serde_json::from_str(&contents)
            .with_context(|| format!("could not parse {}", self.path.display()))?;
        if config.version == 0 {
            config.version = WORKSPACES_VERSION;
        }
        Ok(config)
    }

    pub fn save(&self, config: &WorkspaceConfig) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create directory {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(config)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json).with_context(|| format!("could not write {}", tmp.display()))?;
        set_file_permissions_600(&tmp)?;
        fs::rename(&tmp, &self.path)
            .with_context(|| format!("could not move {}", self.path.display()))?;
        Ok(())
    }
}

pub fn repo_local_config_path(repo: &RepoInfo) -> PathBuf {
    repo.git_dir.join("info").join(REPO_LOCAL_FILE)
}

pub fn load_repo_local_config(repo: &RepoInfo) -> Result<Option<RepoLocalWorkspaceConfig>> {
    let path = repo_local_config_path(repo);
    if !path.exists() {
        return Ok(None);
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
    let mut config: RepoLocalWorkspaceConfig = serde_json::from_str(&contents)
        .with_context(|| format!("could not parse {}", path.display()))?;
    if config.version == 0 {
        config.version = WORKSPACES_VERSION;
    }
    Ok(Some(config))
}

pub fn save_repo_local_config(repo: &RepoInfo, context: &str) -> Result<()> {
    let path = repo_local_config_path(repo);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create directory {}", parent.display()))?;
    }
    let config = RepoLocalWorkspaceConfig {
        version: WORKSPACES_VERSION,
        context: context.to_owned(),
    };
    let json = serde_json::to_string_pretty(&config)?;
    fs::write(&path, json).with_context(|| format!("could not write {}", path.display()))?;
    set_file_permissions_600(&path)?;
    Ok(())
}

pub fn detect_repo(start: &Path) -> Result<Option<RepoInfo>> {
    let mut current = absolutize(start)?;
    loop {
        let dot_git = current.join(".git");
        if dot_git.is_dir() {
            return Ok(Some(RepoInfo {
                root: current.clone(),
                git_dir: dot_git.clone(),
                remotes: load_git_remotes(&dot_git)?,
            }));
        }
        if dot_git.is_file() {
            let git_dir = resolve_gitdir_pointer(&dot_git)?;
            return Ok(Some(RepoInfo {
                root: current.clone(),
                remotes: load_git_remotes(&git_dir)?,
                git_dir,
            }));
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

pub fn resolve_binding(home: &Path, cwd: &Path) -> Result<WorkspaceBinding> {
    let workspace = absolutize(cwd)?;
    let repo = detect_repo(&workspace)?;
    let store = WorkspaceStore::new(home);
    let config = store.load()?;

    if let Some(repo) = repo.as_ref() {
        if let Some(local) = load_repo_local_config(repo)? {
            return Ok(WorkspaceBinding {
                workspace,
                repo_root: Some(repo.root.clone()),
                expected_context: Some(local.context),
                matched_rule: Some(format!(
                    "repo_local:{}",
                    repo_local_config_path(repo).display()
                )),
            });
        }
    }

    let canonical_workspace = fs::canonicalize(&workspace).unwrap_or_else(|_| workspace.clone());
    if let Some(rule) = find_matching_path_rule(&config.path_rules, &canonical_workspace)? {
        return Ok(WorkspaceBinding {
            workspace,
            repo_root: repo.as_ref().map(|r| r.root.clone()),
            expected_context: Some(rule.context.clone()),
            matched_rule: Some(format!("path:{}", rule.path)),
        });
    }

    if let Some(repo) = repo.as_ref() {
        if let Some(rule) = find_matching_remote_rule(&config.git_remote_rules, &repo.remotes) {
            return Ok(WorkspaceBinding {
                workspace,
                repo_root: Some(repo.root.clone()),
                expected_context: Some(rule.context.clone()),
                matched_rule: Some(format!("git_remote:{}", rule.pattern)),
            });
        }
    }

    if let Some(default_context) = config.default_context.clone() {
        return Ok(WorkspaceBinding {
            workspace,
            repo_root: repo.as_ref().map(|r| r.root.clone()),
            expected_context: Some(default_context),
            matched_rule: Some("default".to_owned()),
        });
    }

    Ok(WorkspaceBinding {
        workspace,
        repo_root: repo.as_ref().map(|r| r.root.clone()),
        expected_context: None,
        matched_rule: None,
    })
}

pub fn collect_workspace_status(home: &Path, cwd: &Path) -> Result<WorkspaceStatus> {
    let binding = resolve_binding(home, cwd)?;
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;
    let user_home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let statuses = collect_status(
        home,
        &user_home,
        &std::env::var_os("PATH").unwrap_or_default(),
    )?;
    let context_status = derive_context_status(&config, &statuses);
    let active_profiles = Tool::ALL
        .iter()
        .map(|tool| {
            (
                *tool,
                statuses
                    .iter()
                    .find(|status| status.tool == *tool)
                    .and_then(|status| status.active_profile.clone()),
            )
        })
        .collect::<HashMap<_, _>>();

    let status = classify_workspace_state(&config, &binding, &active_profiles, &context_status);
    let recommended_command = match status {
        WorkspaceState::Mismatch | WorkspaceState::AmbiguousActive | WorkspaceState::Unmanaged => {
            binding
                .expected_context
                .as_ref()
                .map(|expected| format!("aisw context use {expected}"))
        }
        _ => None,
    };

    Ok(WorkspaceStatus {
        workspace: binding.workspace,
        repo_root: binding.repo_root,
        matched_rule: binding.matched_rule,
        expected_context: binding.expected_context,
        active_context: context_status.active.clone(),
        active_profiles,
        status,
        recommended_command,
    })
}

pub fn context_matches_active(
    config: &Config,
    context_name: &str,
    active_profiles: &HashMap<Tool, Option<String>>,
) -> bool {
    let Some(context) = config.context(context_name) else {
        return false;
    };
    let total = context.profiles.iter().count();
    total > 0
        && context.profiles.iter().all(|(tool, profile)| {
            active_profiles
                .get(&tool)
                .and_then(|value| value.as_deref())
                == Some(profile)
        })
}

pub fn guard_mode(home: &Path) -> Result<GuardMode> {
    Ok(WorkspaceStore::new(home).load()?.guard_mode)
}

fn classify_workspace_state(
    config: &Config,
    binding: &WorkspaceBinding,
    active_profiles: &HashMap<Tool, Option<String>>,
    context_status: &DerivedContextStatus,
) -> WorkspaceState {
    let Some(expected_context) = binding.expected_context.as_deref() else {
        return WorkspaceState::NoExpectedContext;
    };
    if config.context(expected_context).is_none() {
        return WorkspaceState::InvalidContext;
    }
    if context_matches_active(config, expected_context, active_profiles) {
        return WorkspaceState::Match;
    }
    if context_status.is_ambiguous() {
        return WorkspaceState::AmbiguousActive;
    }
    if active_profiles.values().all(|profile| profile.is_none()) {
        return WorkspaceState::Unmanaged;
    }
    WorkspaceState::Mismatch
}

fn find_matching_path_rule<'a>(
    rules: &'a [PathRule],
    workspace: &Path,
) -> Result<Option<&'a PathRule>> {
    let mut best: Option<(&PathRule, usize)> = None;
    for rule in rules {
        let path = expand_tilde(Path::new(&rule.path))?;
        let abs = absolutize(&path)?;
        let candidate = fs::canonicalize(&abs).unwrap_or(abs);
        if is_path_prefix(&candidate, workspace) {
            let score = candidate.components().count();
            match best {
                Some((_, best_score)) if best_score >= score => {}
                _ => best = Some((rule, score)),
            }
        }
    }
    Ok(best.map(|(rule, _)| rule))
}

fn find_matching_remote_rule<'a>(
    rules: &'a [GitRemoteRule],
    remotes: &[String],
) -> Option<&'a GitRemoteRule> {
    let mut best: Option<(&GitRemoteRule, usize, usize)> = None;
    for (rule_index, rule) in rules.iter().enumerate() {
        let pattern = normalize_remote_pattern(&rule.pattern);
        if remotes
            .iter()
            .any(|remote| wildcard_match(&pattern, remote))
        {
            let specificity = pattern.chars().filter(|ch| *ch != '*').count();
            match best {
                Some((_, best_specificity, best_index))
                    if best_specificity > specificity
                        || (best_specificity == specificity && best_index < rule_index) => {}
                _ => best = Some((rule, specificity, rule_index)),
            }
        }
    }
    best.map(|(rule, _, _)| rule)
}

pub fn normalize_remote_pattern(pattern: &str) -> String {
    normalize_remote_like(pattern)
}

pub fn normalize_remote(url: &str) -> String {
    normalize_remote_like(url)
}

fn normalize_remote_like(input: &str) -> String {
    let trimmed = input.trim();
    let without_scheme = if let Some(rest) = trimmed.strip_prefix("ssh://") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("https://") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("git://") {
        rest
    } else {
        trimmed
    };

    let without_user = without_scheme
        .strip_prefix("git@")
        .or_else(|| without_scheme.strip_prefix("ssh@"))
        .unwrap_or(without_scheme);

    let normalized = if let Some((host, path)) = without_user.split_once(':') {
        format!("{host}/{path}")
    } else {
        without_user.to_owned()
    };

    normalized
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase()
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let p = pattern.as_bytes();
    let v = value.as_bytes();
    let (mut pi, mut vi) = (0usize, 0usize);
    let (mut star, mut match_i) = (None, 0usize);
    while vi < v.len() {
        if pi < p.len() && p[pi] == v[vi] {
            pi += 1;
            vi += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            match_i = vi;
        } else if let Some(star_idx) = star {
            pi = star_idx + 1;
            match_i += 1;
            vi = match_i;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

fn load_git_remotes(git_dir: &Path) -> Result<Vec<String>> {
    let config_path = git_dir.join("config");
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&config_path)
        .with_context(|| format!("could not read {}", config_path.display()))?;
    let mut in_remote_section = false;
    let mut remotes = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_remote_section = trimmed.starts_with("[remote \"") && trimmed.ends_with("\"]");
            continue;
        }
        if !in_remote_section || !trimmed.starts_with("url") {
            continue;
        }
        let Some((_, value)) = trimmed.split_once('=') else {
            continue;
        };
        remotes.push(normalize_remote(value.trim()));
    }
    Ok(remotes)
}

fn resolve_gitdir_pointer(dot_git_file: &Path) -> Result<PathBuf> {
    let contents = fs::read_to_string(dot_git_file)
        .with_context(|| format!("could not read {}", dot_git_file.display()))?;
    let raw = contents
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)
        .ok_or_else(|| anyhow!("invalid gitdir pointer in {}", dot_git_file.display()))?;
    let git_dir = PathBuf::from(raw);
    if git_dir.is_absolute() {
        Ok(git_dir)
    } else {
        Ok(dot_git_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(git_dir))
    }
}

pub fn expand_tilde(path: &Path) -> Result<PathBuf> {
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first)) if first == "~" => {
            let home = dirs::home_dir().context("could not determine home directory")?;
            let mut expanded = home;
            for component in components {
                expanded.push(component.as_os_str());
            }
            Ok(expanded)
        }
        _ => Ok(path.to_path_buf()),
    }
}

fn absolutize(path: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde(path)?;
    if expanded.is_absolute() {
        return Ok(expanded);
    }
    Ok(std::env::current_dir()
        .context("could not determine current directory")?
        .join(expanded))
}

fn is_path_prefix(prefix: &Path, value: &Path) -> bool {
    let prefix_components = prefix.components().collect::<Vec<_>>();
    let value_components = value.components().collect::<Vec<_>>();
    prefix_components.len() <= value_components.len()
        && prefix_components
            .iter()
            .zip(value_components.iter())
            .all(|(a, b)| a == b)
}

fn set_file_permissions_600(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms)
            .with_context(|| format!("could not set permissions on {}", path.display()))?;
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
    Ok(())
}

pub fn validate_context_exists(config: &Config, name: &str) -> Result<()> {
    if config.context(name).is_none() {
        return Err(AiswError::ContextNotFound {
            name: name.to_owned(),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ContextEntry, ContextProfiles};
    use tempfile::TempDir;

    #[test]
    fn normalize_remote_variants() {
        assert_eq!(
            normalize_remote("git@github.com:acme/api.git"),
            "github.com/acme/api"
        );
        assert_eq!(
            normalize_remote("https://github.com/acme/api.git"),
            "github.com/acme/api"
        );
        assert_eq!(
            normalize_remote("ssh://git@github.com/acme/api.git"),
            "github.com/acme/api"
        );
    }

    #[test]
    fn wildcard_matching_handles_single_star() {
        assert!(wildcard_match("github.com/acme/*", "github.com/acme/api"));
        assert!(!wildcard_match("github.com/acme/*", "github.com/other/api"));
    }

    #[test]
    fn path_prefix_matching_is_component_aware() {
        assert!(is_path_prefix(
            Path::new("/tmp/work"),
            Path::new("/tmp/work/api")
        ));
        assert!(!is_path_prefix(
            Path::new("/tmp/work"),
            Path::new("/tmp/workbench")
        ));
    }

    #[test]
    fn remote_rule_prefers_more_specific_pattern_then_first_defined() {
        let rules = vec![
            GitRemoteRule {
                pattern: "github.com/acme/*".to_owned(),
                context: "broad".to_owned(),
            },
            GitRemoteRule {
                pattern: "github.com/acme/api".to_owned(),
                context: "specific".to_owned(),
            },
            GitRemoteRule {
                pattern: "github.com/acme/api".to_owned(),
                context: "same-specificity-later".to_owned(),
            },
        ];

        let matched = find_matching_remote_rule(&rules, &[String::from("github.com/acme/api")])
            .expect("rule should match");
        assert_eq!(matched.context, "specific");
    }

    #[test]
    fn load_git_remotes_ignores_non_remote_urls_and_missing_config() {
        let temp = TempDir::new().unwrap();
        let git_dir = temp.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        assert!(load_git_remotes(&git_dir).unwrap().is_empty());

        fs::write(
            git_dir.join("config"),
            "[core]\n\trepositoryformatversion = 0\nurl = https://example.com/ignored.git\n[remote \"origin\"]\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n\turl = git@github.com:acme/api.git\n",
        )
        .unwrap();

        assert_eq!(
            load_git_remotes(&git_dir).unwrap(),
            vec![String::from("github.com/acme/api")]
        );
    }

    #[test]
    fn load_git_remotes_does_not_leak_url_from_non_remote_section_after_remote() {
        let temp = TempDir::new().unwrap();
        let git_dir = temp.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(
            git_dir.join("config"),
            concat!(
                "[remote \"origin\"]\n",
                "\turl = git@github.com:acme/api.git\n",
                "[branch \"main\"]\n",
                "\turl = https://should-be-ignored.example.com/repo.git\n",
            ),
        )
        .unwrap();

        assert_eq!(
            load_git_remotes(&git_dir).unwrap(),
            vec![String::from("github.com/acme/api")]
        );
    }

    #[test]
    fn resolve_gitdir_pointer_supports_relative_and_absolute_paths() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let dot_git = repo_root.join(".git");
        fs::write(&dot_git, "gitdir: ../actual.git\n").unwrap();
        assert_eq!(
            resolve_gitdir_pointer(&dot_git).unwrap(),
            repo_root.join("../actual.git")
        );

        let absolute = temp.path().join("absolute.git");
        fs::write(&dot_git, format!("gitdir: {}\n", absolute.display())).unwrap();
        assert_eq!(resolve_gitdir_pointer(&dot_git).unwrap(), absolute);
    }

    #[test]
    fn expand_tilde_expands_home_and_leaves_plain_paths_unchanged() {
        let _temp = TempDir::new().unwrap();
        let home = dirs::home_dir().expect("home directory should be available");
        let expanded = expand_tilde(Path::new("~/clients/acme")).unwrap();
        assert_eq!(expanded, home.join("clients").join("acme"));
        assert_eq!(
            expand_tilde(Path::new("relative/path")).unwrap(),
            PathBuf::from("relative/path")
        );
    }

    #[test]
    fn classify_workspace_state_covers_invalid_ambiguous_and_unmanaged() {
        let mut config = Config::default();
        let mut profiles = ContextProfiles::default();
        profiles.insert(Tool::Claude, "work");
        config.contexts.entries_mut().insert(
            "client-acme".to_owned(),
            ContextEntry::new(profiles, chrono::Utc::now()),
        );

        let invalid_binding = WorkspaceBinding {
            workspace: PathBuf::from("/tmp/project"),
            repo_root: None,
            expected_context: Some("missing".to_owned()),
            matched_rule: None,
        };
        let empty_profiles = HashMap::from([
            (Tool::Claude, None),
            (Tool::Codex, None),
            (Tool::Gemini, None),
        ]);
        let ambiguous = DerivedContextStatus {
            status: "ambiguous",
            active: Some("other".to_owned()),
            matches: vec!["client-acme".to_owned()],
            drift_candidates: vec![],
            mapped_profiles: None,
            unmanaged_tools: vec![],
        };
        assert_eq!(
            classify_workspace_state(&config, &invalid_binding, &empty_profiles, &ambiguous),
            WorkspaceState::InvalidContext
        );

        let valid_binding = WorkspaceBinding {
            workspace: PathBuf::from("/tmp/project"),
            repo_root: None,
            expected_context: Some("client-acme".to_owned()),
            matched_rule: None,
        };
        assert_eq!(
            classify_workspace_state(&config, &valid_binding, &empty_profiles, &ambiguous),
            WorkspaceState::AmbiguousActive
        );

        let unmanaged = DerivedContextStatus {
            status: "unmanaged",
            active: None,
            matches: vec![],
            drift_candidates: vec![],
            mapped_profiles: None,
            unmanaged_tools: vec![],
        };
        assert_eq!(
            classify_workspace_state(&config, &valid_binding, &empty_profiles, &unmanaged),
            WorkspaceState::Unmanaged
        );
    }
}
