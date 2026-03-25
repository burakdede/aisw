// Integration tests for `aisw remove`.
mod common;

use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn add_claude(env: &TestEnv, name: &str) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", name, "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();
}

fn activate_claude(env: &TestEnv, name: &str) {
    env.cmd().args(["use", "claude", name]).assert().success();
}

#[test]
fn remove_profile_exits_zero_and_deletes_dir() {
    let env = TestEnv::new();
    add_claude(&env, "work");

    env.cmd()
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .success()
        .stdout(contains("Removed"));

    // Profile dir should be gone.
    let profile_dir = env.aisw_home.join("profiles").join("claude").join("work");
    assert!(!profile_dir.exists(), "profile dir should be deleted");
}

#[test]
fn remove_profile_no_longer_in_list() {
    let env = TestEnv::new();
    add_claude(&env, "work");

    env.cmd()
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .success();

    env.cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(contains("No profiles found"));
}

#[test]
fn remove_nonexistent_fails_with_not_found() {
    TestEnv::new()
        .cmd()
        .args(["remove", "claude", "ghost", "--yes"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

#[test]
fn remove_active_profile_without_force_fails() {
    let env = TestEnv::new();
    add_claude(&env, "work");
    activate_claude(&env, "work");

    env.cmd()
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .failure()
        .stderr(contains("currently active"));
}

#[test]
fn remove_active_profile_with_force_succeeds_and_clears_active() {
    let env = TestEnv::new();
    add_claude(&env, "work");
    activate_claude(&env, "work");

    env.cmd()
        .args(["remove", "claude", "work", "--yes", "--force"])
        .assert()
        .success()
        .stdout(contains("Removed"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert!(config["active"]["claude"].is_null());
    assert!(config["profiles"]["claude"]["work"].is_null());
}

#[test]
fn remove_creates_backup_before_deletion() {
    let env = TestEnv::new();
    add_claude(&env, "work");

    env.cmd()
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .success();

    let backups_dir = env.home_file("backups");
    assert!(
        backups_dir.exists(),
        "backups dir should exist after removal"
    );
    let entries: Vec<_> = std::fs::read_dir(&backups_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "at least one backup expected");
}
