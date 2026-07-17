use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::cli::VerifyArgs;
use crate::commands::{doctor, status};
use crate::runtime;
use crate::types::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum VerifyStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
struct ToolVerification {
    tool: &'static str,
    status: VerifyStatus,
    active_profile: Option<String>,
    stored_profiles: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    remediation: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct VerifySummary {
    status: VerifyStatus,
    passed: usize,
    warnings: usize,
    failed: usize,
}

#[derive(Debug, Clone, Serialize)]
struct VerifyReport {
    summary: VerifySummary,
    tools: Vec<ToolVerification>,
    doctor: Vec<doctor::CheckResult>,
}

pub fn run(args: VerifyArgs, home: &Path) -> Result<bool> {
    let user_home = dirs::home_dir().unwrap_or_else(|| Path::new(".").to_path_buf());
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    run_in(args, home, &user_home, path_var.as_os_str())
}

pub(crate) fn run_in(
    args: VerifyArgs,
    home: &Path,
    user_home: &Path,
    path_var: &std::ffi::OsStr,
) -> Result<bool> {
    let report = collect(home, user_home, path_var)?;
    let passed = report.summary.status != VerifyStatus::Fail;

    if !runtime::is_quiet() {
        if args.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            print_text(&report);
        }
    }

    Ok(passed)
}

fn collect(home: &Path, user_home: &Path, path_var: &std::ffi::OsStr) -> Result<VerifyReport> {
    let doctor_report = doctor::collect(home, user_home, path_var);
    let tool_statuses = status::collect_status(home, user_home, &path_var.to_os_string())?;
    let tools = tool_statuses
        .iter()
        .map(tool_verification)
        .collect::<Vec<_>>();
    let summary = summarize(&doctor_report.checks, &tools);

    Ok(VerifyReport {
        summary,
        tools,
        doctor: doctor_report.checks,
    })
}

fn tool_verification(tool: &status::ToolStatus) -> ToolVerification {
    let mut issues = Vec::new();
    let mut remediation = Vec::new();
    let status = if !tool.binary_found {
        issues.push("tool binary not found on PATH".to_owned());
        remediation.push(format!(
            "Install {} or run 'aisw doctor --json'",
            tool.tool.binary_name()
        ));
        VerifyStatus::Fail
    } else if tool.active_profile.is_none() {
        if tool.stored_profiles > 0 {
            issues.push("profiles are stored, but no active profile is selected".to_owned());
            remediation.push(format!(
                "Run 'aisw use {} <profile>'",
                tool.tool.binary_name()
            ));
            VerifyStatus::Warn
        } else {
            issues.push("no managed profiles stored".to_owned());
            remediation.push(format!(
                "Run 'aisw add {} <profile>' or import with 'aisw init --json --no-shell-hook --detect-live'",
                tool.tool.binary_name()
            ));
            VerifyStatus::Warn
        }
    } else if !tool.credentials_present {
        issues.push("managed credentials are missing".to_owned());
        remediation.push(format!(
            "Restore the profile from backup or re-run 'aisw add {} {}'",
            tool.tool.binary_name(),
            tool.active_profile.as_deref().unwrap_or("<profile>")
        ));
        VerifyStatus::Fail
    } else if !tool.permissions_ok {
        issues.push("managed credential permissions are too broad".to_owned());
        remediation
            .push("Run 'aisw doctor --json' to inspect the failing permission check".to_owned());
        VerifyStatus::Fail
    } else if tool.active_profile_applied == Some(false) {
        issues.push("live tool credentials do not match the recorded active profile".to_owned());
        remediation.push(format!(
            "Run 'aisw use {} {}' to reapply the active profile",
            tool.tool.binary_name(),
            tool.active_profile.as_deref().unwrap_or("<profile>")
        ));
        VerifyStatus::Fail
    } else if tool.tool == Tool::Claude
        && tool.credential_backend.as_deref() == Some("file")
        && cfg!(target_os = "macos")
        && tool.active_profile_applied.is_none()
    {
        issues.push(
            "live macOS Keychain state was not verified for this Claude file-backed profile"
                .to_owned(),
        );
        remediation.push("Use 'aisw status --json' for the current live-match signal, or switch once with 'aisw use claude <profile>'.".to_owned());
        VerifyStatus::Warn
    } else {
        VerifyStatus::Pass
    };

    ToolVerification {
        tool: tool.tool.binary_name(),
        status,
        active_profile: tool.active_profile.clone(),
        stored_profiles: tool.stored_profiles,
        issues,
        remediation,
    }
}

fn summarize(doctor_checks: &[doctor::CheckResult], tools: &[ToolVerification]) -> VerifySummary {
    let mut passed = 0usize;
    let mut warnings = 0usize;
    let mut failed = 0usize;

    for check in doctor_checks {
        match check.status {
            doctor::CheckStatus::Pass => passed += 1,
            doctor::CheckStatus::Warn => warnings += 1,
            doctor::CheckStatus::Fail => failed += 1,
        }
    }

    for tool in tools {
        match tool.status {
            VerifyStatus::Pass => passed += 1,
            VerifyStatus::Warn => warnings += 1,
            VerifyStatus::Fail => failed += 1,
        }
    }

    let status = if failed > 0 {
        VerifyStatus::Fail
    } else if warnings > 0 {
        VerifyStatus::Warn
    } else {
        VerifyStatus::Pass
    };

    VerifySummary {
        status,
        passed,
        warnings,
        failed,
    }
}

fn print_text(report: &VerifyReport) {
    crate::output::print_title("Verify");
    crate::output::print_kv(
        "Summary",
        match report.summary.status {
            VerifyStatus::Pass => "pass",
            VerifyStatus::Warn => "warn",
            VerifyStatus::Fail => "fail",
        },
    );
    crate::output::print_kv("Passed", report.summary.passed.to_string());
    crate::output::print_kv("Warnings", report.summary.warnings.to_string());
    crate::output::print_kv("Failed", report.summary.failed.to_string());
    crate::output::print_blank_line();

    for tool in &report.tools {
        crate::output::print_tool_section(match tool.tool {
            "claude" => Tool::Claude,
            "codex" => Tool::Codex,
            _ => Tool::Gemini,
        });
        crate::output::print_kv(
            "Status",
            match tool.status {
                VerifyStatus::Pass => "pass",
                VerifyStatus::Warn => "warn",
                VerifyStatus::Fail => "fail",
            },
        );
        crate::output::print_kv("Active", tool.active_profile.as_deref().unwrap_or("none"));
        for issue in &tool.issues {
            crate::output::print_info(issue);
        }
        for fix in &tool.remediation {
            crate::output::print_fix(fix);
        }
        crate::output::print_blank_line();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_status(tool: Tool) -> status::ToolStatus {
        status::ToolStatus {
            tool,
            binary_found: true,
            stored_profiles: 1,
            active_profile: Some("work".to_owned()),
            auth_method: Some("api_key".to_owned()),
            credential_backend: Some("file".to_owned()),
            claude_auth_classification: None,
            codex_auth_classification: None,
            antigravity_auth_classification: None,
            state_mode: Some("isolated".to_owned()),
            active_profile_added_at: None,
            active_profile_applied: Some(true),
            credentials_present: true,
            permissions_ok: true,
        }
    }

    #[test]
    fn verify_fails_when_tool_binary_is_missing() {
        let mut tool = tool_status(Tool::Gemini);
        tool.binary_found = false;

        let result = tool_verification(&tool);

        assert_eq!(result.status, VerifyStatus::Fail);
        assert!(result.issues[0].contains("binary"));
        assert!(result.remediation[0].contains("aisw doctor --json"));
    }

    #[test]
    fn verify_warns_when_profiles_exist_but_none_active() {
        let mut tool = tool_status(Tool::Claude);
        tool.active_profile = None;
        tool.auth_method = None;
        tool.credential_backend = None;
        tool.active_profile_applied = None;
        tool.credentials_present = false;

        let claude = tool_verification(&tool);
        assert_eq!(claude.status, VerifyStatus::Warn);
        assert!(claude.issues[0].contains("no active profile"));
    }

    #[test]
    fn verify_warns_when_no_profiles_are_stored() {
        let mut tool = tool_status(Tool::Claude);
        tool.stored_profiles = 0;
        tool.active_profile = None;
        tool.auth_method = None;
        tool.credential_backend = None;
        tool.active_profile_applied = None;
        tool.credentials_present = false;

        let result = tool_verification(&tool);

        assert_eq!(result.status, VerifyStatus::Warn);
        assert!(result.issues[0].contains("no managed profiles"));
        assert!(result.remediation[0].contains("aisw add claude"));
    }

    #[test]
    fn verify_fails_when_managed_credentials_are_missing() {
        let mut tool = tool_status(Tool::Codex);
        tool.credentials_present = false;

        let result = tool_verification(&tool);

        assert_eq!(result.status, VerifyStatus::Fail);
        assert!(result.issues[0].contains("managed credentials are missing"));
    }

    #[test]
    fn verify_fails_when_permissions_are_too_broad() {
        let mut tool = tool_status(Tool::Codex);
        tool.permissions_ok = false;

        let result = tool_verification(&tool);

        assert_eq!(result.status, VerifyStatus::Fail);
        assert!(result.issues[0].contains("permissions"));
    }

    #[test]
    fn verify_fails_when_live_state_mismatches_active_profile() {
        let mut tool = tool_status(Tool::Codex);
        tool.active_profile_applied = Some(false);

        let codex = tool_verification(&tool);
        assert_eq!(codex.status, VerifyStatus::Fail);
        assert!(codex.issues[0].contains("live tool credentials do not match"));
    }

    #[test]
    fn verify_passes_when_active_profile_matches_live_state() {
        let result = tool_verification(&tool_status(Tool::Codex));

        assert_eq!(result.status, VerifyStatus::Pass);
        assert!(result.issues.is_empty());
        assert!(result.remediation.is_empty());
    }

    #[test]
    fn summarize_counts_pass_warn_and_fail() {
        let summary = summarize(
            &[
                doctor::CheckResult {
                    name: "config".to_owned(),
                    status: doctor::CheckStatus::Pass,
                    detail: "ok".to_owned(),
                },
                doctor::CheckResult {
                    name: "shell".to_owned(),
                    status: doctor::CheckStatus::Warn,
                    detail: "warn".to_owned(),
                },
            ],
            &[
                ToolVerification {
                    tool: "claude",
                    status: VerifyStatus::Pass,
                    active_profile: Some("work".to_owned()),
                    stored_profiles: 1,
                    issues: Vec::new(),
                    remediation: Vec::new(),
                },
                ToolVerification {
                    tool: "codex",
                    status: VerifyStatus::Fail,
                    active_profile: Some("work".to_owned()),
                    stored_profiles: 1,
                    issues: vec!["mismatch".to_owned()],
                    remediation: vec!["aisw use codex work".to_owned()],
                },
            ],
        );

        assert_eq!(summary.status, VerifyStatus::Fail);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.warnings, 1);
        assert_eq!(summary.failed, 1);
    }
}
