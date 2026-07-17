mod common;

use common::TestEnv;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const ANTIGRAVITY_SECRET: &str = r#"{"email":"work@example.com","token":"work-live"}"#;

fn add_claude_profile(env: &TestEnv, name: &str, key: &str) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", name, "--api-key", key])
        .assert()
        .success();
}

fn write_antigravity_live_state(env: &TestEnv, secret: &str) {
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
    std::fs::write(secret_path, secret).unwrap();
}

fn add_antigravity_profile(env: &TestEnv, name: &str) {
    env.add_fake_tool("agy", "agy 1.0.0");
    write_antigravity_live_state(env, ANTIGRAVITY_SECRET);
    env.cmd()
        .args(["add", "antigravity", name, "--from-live"])
        .assert()
        .success();
}

#[test]
fn rename_profile_updates_list_output() {
    let env = TestEnv::new();
    add_claude_profile(&env, "default", VALID_CLAUDE_KEY);

    env.cmd()
        .args(["rename", "claude", "default", "work"])
        .assert()
        .success()
        .stdout(contains("Renamed profile"))
        .stdout(contains("Claude Code"))
        .stdout(contains("default"))
        .stdout(contains("work"));

    env.cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(contains("work"))
        .stdout(predicates::str::contains("default").not());
}

#[test]
fn rename_active_profile_preserves_active_state() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args([
            "add",
            "claude",
            "default",
            "--api-key",
            VALID_CLAUDE_KEY,
            "--set-active",
        ])
        .assert()
        .success();

    env.cmd()
        .args(["rename", "claude", "default", "work"])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "work");
}

#[test]
fn rename_duplicate_target_fails() {
    let env = TestEnv::new();
    add_claude_profile(&env, "default", VALID_CLAUDE_KEY);
    add_claude_profile(&env, "work", VALID_CLAUDE_KEY_ALT);

    env.cmd()
        .args(["rename", "claude", "default", "work"])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

#[test]
fn rename_without_old_name_in_non_tty_fails_clearly() {
    let env = TestEnv::new();
    add_claude_profile(&env, "default", VALID_CLAUDE_KEY);

    env.cmd()
        .args(["rename", "claude", "work"])
        .assert()
        .failure()
        .stderr(contains("requires an interactive TTY"))
        .stderr(contains("aisw rename claude <old> <new>"));
}

#[test]
fn rename_antigravity_profile_preserves_active_state_and_updates_list() {
    let env = TestEnv::new();
    add_antigravity_profile(&env, "default");

    env.cmd()
        .args(["rename", "antigravity", "default", "work"])
        .assert()
        .success()
        .stdout(contains("Renamed profile"))
        .stdout(contains("Antigravity CLI"))
        .stdout(contains("default"))
        .stdout(contains("work"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["antigravity"], "work");

    env.cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(contains("work"))
        .stdout(predicates::str::contains("default").not());
}
