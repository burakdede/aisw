mod common;

use common::TestEnv;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn add_claude_profile(env: &TestEnv, name: &str) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", name, "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();
}

#[test]
fn rename_profile_updates_list_output() {
    let env = TestEnv::new();
    add_claude_profile(&env, "default");

    env.cmd()
        .args(["rename", "claude", "default", "work"])
        .assert()
        .success()
        .stdout(contains("Renamed claude profile 'default' to 'work'."));

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
    add_claude_profile(&env, "default");
    add_claude_profile(&env, "work");

    env.cmd()
        .args(["rename", "claude", "default", "work"])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}
