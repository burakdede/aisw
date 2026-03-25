// Integration tests for `aisw status`.
mod common;

use std::os::unix::fs::PermissionsExt;

use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn add_and_activate_claude(env: &TestEnv, name: &str) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", name, "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();
    env.cmd().args(["use", "claude", name]).assert().success();
}

#[test]
fn status_no_profiles_no_tools_exits_zero() {
    // Empty PATH → no tools found.
    TestEnv::new()
        .cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("binary not found"));
}

#[test]
fn status_shows_credentials_present_for_active_profile() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "work");

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("work"))
        .stdout(contains("credentials present"));
}

#[test]
fn status_json_has_expected_keys() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "work");

    let output = env
        .cmd()
        .args(["status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).expect("invalid JSON");
    assert!(json.is_array());
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 3); // one entry per tool

    let claude = arr.iter().find(|e| e["tool"] == "claude").unwrap();
    assert_eq!(claude["binary_found"], true);
    assert_eq!(claude["stored_profiles"], 1);
    assert_eq!(claude["active_profile"], "work");
    assert_eq!(claude["effective_in_current_session"], false);
    assert_eq!(claude["credentials_present"], true);
    assert_eq!(claude["permissions_ok"], true);
}

#[test]
fn status_warns_on_broad_permissions() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "work");

    // Widen permissions on the credentials file.
    let cred = env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("work")
        .join(".credentials.json");
    std::fs::set_permissions(&cred, std::fs::Permissions::from_mode(0o644)).unwrap();

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("permissions too broad"));
}

#[test]
fn status_reports_shell_mismatch_for_active_claude_profile() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "work");

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("current shell is not using this profile"));
}

#[test]
fn status_reports_credentials_present_when_current_shell_matches() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "work");

    env.cmd()
        .args(["status"])
        .env(
            "ANTHROPIC_API_KEY",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .assert()
        .success()
        .stdout(contains("credentials present (validity not checked)"));
}

#[test]
fn status_no_active_profile_shows_dash() {
    let env = TestEnv::new();
    // Add a profile but don't activate it.
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("profiles stored, but none is active"));
}
