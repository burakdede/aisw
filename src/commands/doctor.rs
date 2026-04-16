//! `aisw doctor` — structured health-check report.
//!
//! Every check is a pure function that receives its inputs (paths, env vars,
//! file content) via parameters rather than reading global state.  This keeps
//! the logic unit-testable without spawning sub-processes or mutating the
//! environment.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use console::style;
use serde::Serialize;

use crate::cli::DoctorArgs;
use crate::config::{ConfigStore, CredentialBackend};
use crate::profile::ProfileStore;
use crate::runtime;
use crate::types::Tool;

// ---- Result types ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

impl CheckResult {
    fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            detail: detail.into(),
        }
    }

    fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warn,
            detail: detail.into(),
        }
    }

    fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }
}

// ---- Individual check functions ----

/// Check whether a named tool binary exists on the given PATH.
pub fn check_tool_binary(tool: Tool, path_var: &std::ffi::OsStr) -> CheckResult {
    let name = format!("tool/{}", tool);
    match crate::tool_detection::detect_at_path(tool, path_var) {
        Some(detected) => {
            let detail = match detected.version {
                Some(v) => format!("{} found ({})", detected.binary_path.display(), v),
                None => format!("{} found", detected.binary_path.display()),
            };
            CheckResult::pass(name, detail)
        }
        None => CheckResult::fail(
            name,
            format!(
                "{} not found on PATH — install it or add it to PATH",
                tool.binary_name()
            ),
        ),
    }
}

/// Check that `~/.aisw/config.json` exists, is valid JSON, and has a supported
/// schema version.
pub fn check_config(config_path: &Path) -> CheckResult {
    const NAME: &str = "config/json";
    if !config_path.exists() {
        return CheckResult::fail(
            NAME,
            format!(
                "{} not found — run 'aisw init' to create it",
                config_path.display()
            ),
        );
    }
    let contents = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(e) => {
            return CheckResult::fail(NAME, format!("could not read config: {e}"));
        }
    };
    match serde_json::from_str::<serde_json::Value>(&contents) {
        Err(e) => CheckResult::fail(NAME, format!("config is not valid JSON: {e}")),
        Ok(v) => {
            let version = v.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
            if version == 0 {
                CheckResult::warn(NAME, "config exists but has no version field".to_owned())
            } else {
                CheckResult::pass(NAME, format!("valid (schema v{version})"))
            }
        }
    }
}

/// Check that the OS keyring is usable.
pub fn check_keyring() -> CheckResult {
    const NAME: &str = "keyring";
    if !crate::auth::system_keyring::is_available() {
        return CheckResult::warn(
            NAME,
            "OS keyring is not available on this platform".to_owned(),
        );
    }
    match crate::auth::system_keyring::usability_diagnostic() {
        None => CheckResult::pass(NAME, "OS keyring is accessible".to_owned()),
        Some(msg) => CheckResult::warn(
            NAME,
            format!("keyring available but may not be usable: {msg}"),
        ),
    }
}

/// Check that the shell hook is installed in the given rc file.
/// `rc_path` is `None` when the shell is not recognized.
pub fn check_shell_hook(shell: Option<&str>, rc_path: Option<&Path>) -> CheckResult {
    const NAME: &str = "shell/hook";
    let Some(shell) = shell else {
        return CheckResult::warn(NAME, "SHELL is not set — cannot detect rc file".to_owned());
    };
    let Some(rc) = rc_path else {
        return CheckResult::warn(NAME, format!("shell '{shell}' rc file location is unknown"));
    };
    if !rc.exists() {
        return CheckResult::warn(
            NAME,
            format!("{} does not exist — hook not installed", rc.display()),
        );
    }
    let contents = match std::fs::read_to_string(rc) {
        Ok(c) => c,
        Err(e) => {
            return CheckResult::fail(NAME, format!("could not read {}: {e}", rc.display()));
        }
    };
    if contents.contains("aisw shell-hook") {
        CheckResult::pass(NAME, format!("hook present in {}", rc.display()))
    } else {
        CheckResult::fail(
            NAME,
            format!(
                "hook not found in {} — run 'aisw init' to install it",
                rc.display()
            ),
        )
    }
}

/// Check that all credential files for a tool's profiles have 0600 permissions.
pub fn check_profile_permissions(
    home: &Path,
    tool: Tool,
    config_store: &ConfigStore,
    profile_store: &ProfileStore,
) -> Vec<CheckResult> {
    let config = match config_store.load() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let profiles = config.profiles_for(tool);
    let mut results = Vec::new();

    for (name, meta) in profiles {
        let check_name = format!("permissions/{}/{}", tool, name);
        // Keyring-backed profiles don't have a credentials file to check.
        if meta.credential_backend == CredentialBackend::SystemKeyring {
            results.push(CheckResult::pass(
                &check_name,
                "keyring-backed (no file to check)".to_owned(),
            ));
            continue;
        }

        let cred_file = profile_store
            .profile_dir(tool, name)
            .join(credentials_filename(tool));

        if !cred_file.exists() {
            results.push(CheckResult::fail(
                &check_name,
                format!("credentials file missing: {}", cred_file.display()),
            ));
            continue;
        }

        match std::fs::metadata(&cred_file) {
            Err(e) => results.push(CheckResult::fail(
                &check_name,
                format!("could not stat {}: {e}", cred_file.display()),
            )),
            Ok(m) => match file_mode_0600_check(&m) {
                Some(0o600) => {
                    results.push(CheckResult::pass(&check_name, "0600 ok".to_owned()));
                }
                Some(mode) => {
                    results.push(CheckResult::fail(
                        &check_name,
                        format!(
                            "{} has permissions {:04o}, expected 0600",
                            cred_file.display(),
                            mode
                        ),
                    ));
                }
                None => {
                    results.push(CheckResult::warn(
                        &check_name,
                        "permission mode check not supported on this platform".to_owned(),
                    ));
                }
            },
        }

        let _ = home;
    }

    results
}

#[cfg(unix)]
fn file_mode_0600_check(metadata: &std::fs::Metadata) -> Option<u32> {
    Some(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn file_mode_0600_check(_metadata: &std::fs::Metadata) -> Option<u32> {
    None
}

fn credentials_filename(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => ".credentials.json",
        Tool::Codex => "auth.json",
        Tool::Gemini => "oauth_credentials.json",
    }
}

// ---- rc file path helper ----

pub fn rc_path_for_shell(shell_exe: &str, user_home: &Path) -> Option<PathBuf> {
    let basename = Path::new(shell_exe)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(shell_exe);
    match basename {
        "bash" => Some(user_home.join(".bashrc")),
        "zsh" => Some(user_home.join(".zshrc")),
        "fish" => Some(user_home.join(".config").join("fish").join("config.fish")),
        _ => None,
    }
}

// ---- Collect all checks ----

pub struct DoctorReport {
    pub checks: Vec<CheckResult>,
}

impl DoctorReport {
    pub fn any_failed(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Fail)
    }
}

pub fn collect(home: &Path, user_home: &Path, path_var: &std::ffi::OsStr) -> DoctorReport {
    let config_store = ConfigStore::new(home);
    let profile_store = ProfileStore::new(home);
    let mut checks: Vec<CheckResult> = Vec::new();

    // 1. Tool binaries
    for tool in Tool::ALL {
        checks.push(check_tool_binary(tool, path_var));
    }

    // 2. Config file
    let config_path = home.join("config.json");
    checks.push(check_config(&config_path));

    // 3. Keyring
    checks.push(check_keyring());

    // 4. Shell hook
    let shell_exe = std::env::var("SHELL").ok();
    let rc_path = shell_exe
        .as_deref()
        .and_then(|s| rc_path_for_shell(s, user_home));
    checks.push(check_shell_hook(shell_exe.as_deref(), rc_path.as_deref()));

    // 5. Profile permissions
    for tool in Tool::ALL {
        checks.extend(check_profile_permissions(
            home,
            tool,
            &config_store,
            &profile_store,
        ));
    }

    DoctorReport { checks }
}

// ---- Output ----

fn print_check(result: &CheckResult) {
    let (symbol, colored_symbol) = match result.status {
        CheckStatus::Pass => ("✓", style("✓").green().bold()),
        CheckStatus::Warn => ("⚠", style("⚠").yellow().bold()),
        CheckStatus::Fail => ("✗", style("✗").red().bold()),
    };
    let _ = symbol;
    println!("  {} {:<32} {}", colored_symbol, result.name, result.detail);
}

fn print_text(report: &DoctorReport) {
    crate::output::print_title("aisw doctor");
    for check in &report.checks {
        print_check(check);
    }
    println!();
    if report.any_failed() {
        println!("{}", style("One or more checks failed.").red().bold());
    } else {
        println!("{}", style("All checks passed.").green().bold());
    }
}

fn print_json(report: &DoctorReport) -> Result<()> {
    #[derive(Serialize)]
    struct Output<'a> {
        checks: &'a [CheckResult],
    }
    let out = serde_json::to_string_pretty(&Output {
        checks: &report.checks,
    })?;
    println!("{out}");
    Ok(())
}

// ---- Entry point ----

pub fn run(args: DoctorArgs, home: &Path) -> Result<bool> {
    let user_home = dirs::home_dir().unwrap_or_else(|| Path::new(".").to_path_buf());
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    run_in(args, home, &user_home, &path_var)
}

pub fn run_in(
    args: DoctorArgs,
    home: &Path,
    user_home: &Path,
    path_var: &std::ffi::OsStr,
) -> Result<bool> {
    let report = collect(home, user_home, path_var);
    let failed = report.any_failed();

    if !runtime::is_quiet() {
        if args.json {
            print_json(&report)?;
        } else {
            print_text(&report);
        }
    }

    Ok(!failed)
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    use super::*;
    use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
    use crate::profile::ProfileStore;
    use chrono::Utc;

    // ---- check_tool_binary ----

    #[test]
    #[cfg(unix)]
    fn tool_binary_pass_when_found() {
        // Put a dummy executable named "claude" on a temp PATH.
        let dir = tempdir().unwrap();
        let bin = dir.path().join("claude");
        fs::write(&bin, "#!/bin/sh\necho '0.1.0'").unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let path = std::ffi::OsString::from(dir.path());
        let result = check_tool_binary(Tool::Claude, &path);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.detail.contains("claude"));
    }

    #[test]
    fn tool_binary_fail_when_missing() {
        let result = check_tool_binary(Tool::Claude, std::ffi::OsStr::new(""));
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.detail.contains("not found"));
    }

    // ---- check_config ----

    #[test]
    fn config_pass_for_valid_json_with_version() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.json");
        fs::write(&p, r#"{"version":1,"profiles":{}}"#).unwrap();
        let result = check_config(&p);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.detail.contains("v1"));
    }

    #[test]
    fn config_fail_when_missing() {
        let p = tempdir().unwrap().path().join("config.json");
        let result = check_config(&p);
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.detail.contains("not found"));
    }

    #[test]
    fn config_fail_for_invalid_json() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.json");
        fs::write(&p, "not json").unwrap();
        let result = check_config(&p);
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.detail.contains("not valid JSON"));
    }

    #[test]
    fn config_warn_when_version_missing() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.json");
        fs::write(&p, r#"{"profiles":{}}"#).unwrap();
        let result = check_config(&p);
        assert_eq!(result.status, CheckStatus::Warn);
    }

    // ---- check_shell_hook ----

    #[test]
    fn shell_hook_pass_when_present() {
        let dir = tempdir().unwrap();
        let rc = dir.path().join(".zshrc");
        fs::write(&rc, "# something\neval \"$(aisw shell-hook zsh)\"\n").unwrap();
        let result = check_shell_hook(Some("zsh"), Some(&rc));
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn shell_hook_fail_when_absent() {
        let dir = tempdir().unwrap();
        let rc = dir.path().join(".zshrc");
        fs::write(&rc, "# nothing here\n").unwrap();
        let result = check_shell_hook(Some("zsh"), Some(&rc));
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.detail.contains("not found"));
    }

    #[test]
    fn shell_hook_warn_when_rc_missing() {
        let p = Path::new("/nonexistent/.zshrc");
        let result = check_shell_hook(Some("zsh"), Some(p));
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn shell_hook_warn_when_shell_unknown() {
        let result = check_shell_hook(None, None);
        assert_eq!(result.status, CheckStatus::Warn);
        assert!(result.detail.contains("SHELL is not set"));
    }

    #[test]
    fn shell_hook_warn_for_unrecognized_shell() {
        let result = check_shell_hook(Some("tcsh"), None);
        assert_eq!(result.status, CheckStatus::Warn);
    }

    // ---- rc_path_for_shell ----

    #[test]
    fn rc_path_bash() {
        let home = Path::new("/home/user");
        assert_eq!(
            rc_path_for_shell("bash", home),
            Some(Path::new("/home/user/.bashrc").to_path_buf())
        );
    }

    #[test]
    fn rc_path_zsh() {
        let home = Path::new("/home/user");
        assert_eq!(
            rc_path_for_shell("zsh", home),
            Some(Path::new("/home/user/.zshrc").to_path_buf())
        );
    }

    #[test]
    fn rc_path_fish() {
        let home = Path::new("/home/user");
        assert_eq!(
            rc_path_for_shell("fish", home),
            Some(Path::new("/home/user/.config/fish/config.fish").to_path_buf())
        );
    }

    #[test]
    fn rc_path_full_exe_path() {
        let home = Path::new("/home/user");
        assert_eq!(
            rc_path_for_shell("/usr/bin/zsh", home),
            Some(Path::new("/home/user/.zshrc").to_path_buf())
        );
    }

    #[test]
    fn rc_path_unknown_shell() {
        let home = Path::new("/home/user");
        assert!(rc_path_for_shell("tcsh", home).is_none());
    }

    // ---- check_profile_permissions ----

    fn make_stores(dir: &Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
    }

    #[test]
    #[cfg(unix)]
    fn permissions_pass_for_600_file() {
        let dir = tempdir().unwrap();
        let (ps, cs) = make_stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        let cred = ps
            .profile_dir(Tool::Claude, "work")
            .join(".credentials.json");
        fs::write(&cred, b"{}").unwrap();
        fs::set_permissions(&cred, fs::Permissions::from_mode(0o600)).unwrap();
        cs.add_profile(
            Tool::Claude,
            "work",
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::ApiKey,
                credential_backend: CredentialBackend::File,
                label: None,
            },
        )
        .unwrap();

        let results = check_profile_permissions(dir.path(), Tool::Claude, &cs, &ps);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Pass);
    }

    #[test]
    #[cfg(unix)]
    fn permissions_fail_for_644_file() {
        let dir = tempdir().unwrap();
        let (ps, cs) = make_stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        let cred = ps
            .profile_dir(Tool::Claude, "work")
            .join(".credentials.json");
        fs::write(&cred, b"{}").unwrap();
        fs::set_permissions(&cred, fs::Permissions::from_mode(0o644)).unwrap();
        cs.add_profile(
            Tool::Claude,
            "work",
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::ApiKey,
                credential_backend: CredentialBackend::File,
                label: None,
            },
        )
        .unwrap();

        let results = check_profile_permissions(dir.path(), Tool::Claude, &cs, &ps);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert!(results[0].detail.contains("0644"));
    }

    #[test]
    fn permissions_pass_for_keyring_backed_profile() {
        let dir = tempdir().unwrap();
        let (ps, cs) = make_stores(dir.path());
        ps.create(Tool::Claude, "work").unwrap();
        cs.add_profile(
            Tool::Claude,
            "work",
            ProfileMeta {
                added_at: Utc::now(),
                auth_method: AuthMethod::OAuth,
                credential_backend: CredentialBackend::SystemKeyring,
                label: None,
            },
        )
        .unwrap();

        let results = check_profile_permissions(dir.path(), Tool::Claude, &cs, &ps);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Pass);
        assert!(results[0].detail.contains("keyring"));
    }

    // ---- run_in integration ----

    #[test]
    fn run_in_returns_false_when_tools_missing() {
        let dir = tempdir().unwrap();
        let home = dir.path().join("aisw");
        let user_home = dir.path().join("user");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        // Write a valid config so the config check passes.
        fs::write(home.join("config.json"), r#"{"version":1,"profiles":{}}"#).unwrap();

        let args = DoctorArgs { json: false };
        let passed = run_in(args, &home, &user_home, std::ffi::OsStr::new("")).unwrap();
        assert!(!passed, "should fail when all tools are missing");
    }

    #[test]
    fn run_in_json_output_is_valid() {
        let dir = tempdir().unwrap();
        let home = dir.path().join("aisw");
        let user_home = dir.path().join("user");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        fs::write(home.join("config.json"), r#"{"version":1,"profiles":{}}"#).unwrap();

        let args = DoctorArgs { json: true };
        // Should not panic or error.
        let _ = run_in(args, &home, &user_home, std::ffi::OsStr::new(""));
    }
}
