// Integration tests for `aisw remove`.
mod common;

use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const ANTIGRAVITY_SECRET: &str = r#"{"email":"work@example.com","token":"work-live"}"#;

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

fn add_antigravity(env: &TestEnv, name: &str) {
    env.add_fake_tool("agy", "agy 1.0.0");
    let app_dir = env.fake_home.join(".gemini").join("antigravity-cli");
    let shared_dir = env.fake_home.join(".gemini").join("config");
    std::fs::create_dir_all(app_dir.join("cache")).unwrap();
    std::fs::create_dir_all(shared_dir.join("projects")).unwrap();
    std::fs::write(app_dir.join("settings.json"), br#"{"theme":"terminal"}"#).unwrap();
    std::fs::write(
        app_dir.join("cache").join("projects.json"),
        br#"{"current":"repo"}"#,
    )
    .unwrap();
    std::fs::write(shared_dir.join("hooks.json"), br#"{"hooks":["plan"]}"#).unwrap();
    std::fs::write(
        shared_dir.join("projects").join("repo.json"),
        br#"{"mode":"plan"}"#,
    )
    .unwrap();
    let secret_path = env
        .fake_home
        .join("keychain")
        .join("gemini")
        .join("antigravity")
        .join("secret");
    std::fs::create_dir_all(secret_path.parent().unwrap()).unwrap();
    std::fs::write(secret_path.parent().unwrap().join("account"), "antigravity").unwrap();
    std::fs::write(secret_path, ANTIGRAVITY_SECRET).unwrap();

    env.cmd()
        .args(["add", "antigravity", name, "--from-live"])
        .assert()
        .success();
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

#[test]
fn remove_non_interactive_without_yes_fails_clearly() {
    let env = TestEnv::new();
    add_claude(&env, "work");

    env.cmd()
        .args(["--non-interactive", "remove", "claude", "work"])
        .assert()
        .failure()
        .stderr(contains("requires confirmation"))
        .stderr(contains("--yes"));
}

#[test]
fn remove_decline_prompt_exits_nonzero() {
    let env = TestEnv::new();
    add_claude(&env, "work");

    env.cmd()
        .args(["remove", "claude", "work"])
        .write_stdin("n\n")
        .assert()
        .failure()
        .stderr(contains("operation cancelled by user"));
}

#[test]
fn remove_without_profile_in_non_tty_fails_clearly() {
    let env = TestEnv::new();
    add_claude(&env, "work");

    env.cmd()
        .args(["remove", "claude", "--yes"])
        .assert()
        .failure()
        .stderr(contains("requires an interactive TTY"))
        .stderr(contains("aisw remove claude <profile>"));
}

#[test]
fn remove_active_antigravity_profile_with_force_clears_active_and_deletes_dir() {
    let env = TestEnv::new();
    add_antigravity(&env, "work");

    env.cmd()
        .args(["remove", "antigravity", "work", "--yes", "--force"])
        .assert()
        .success()
        .stdout(contains("Removed"))
        .stdout(contains("Antigravity CLI"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert!(config["active"]["antigravity"].is_null());
    assert!(config["profiles"]["antigravity"]["work"].is_null());
    assert!(!env
        .aisw_home
        .join("profiles")
        .join("antigravity")
        .join("work")
        .exists());
}
