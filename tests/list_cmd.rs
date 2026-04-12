// Integration tests for `aisw list`.
mod common;

use common::TestEnv;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";

fn add_claude(env: &TestEnv, name: &str) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", name, "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();
}

fn add_codex(env: &TestEnv, name: &str) {
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "codex", name, "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();
}

#[test]
fn list_no_profiles_exits_zero_with_empty_message() {
    TestEnv::new()
        .cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(contains("No profiles found"));
}

#[test]
fn list_shows_added_profiles() {
    let env = TestEnv::new();
    add_claude(&env, "work");
    add_codex(&env, "main");

    env.cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(contains("Claude Code"))
        .stdout(contains("work"))
        .stdout(contains("Codex CLI"))
        .stdout(contains("main"))
        .stdout(contains("api-key"));
}

#[test]
fn list_filters_by_tool() {
    let env = TestEnv::new();
    add_claude(&env, "work");
    add_codex(&env, "main");

    // Only claude
    env.cmd()
        .args(["list", "claude"])
        .assert()
        .success()
        .stdout(contains("work"))
        .stdout(contains("Claude Code"));

    // codex profile should NOT appear
    env.cmd()
        .args(["list", "claude"])
        .assert()
        .stdout(predicates::str::contains("main").not());
}

#[test]
fn list_json_output_is_valid_json_object() {
    let env = TestEnv::new();
    add_claude(&env, "work");

    let output = env
        .cmd()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("stdout is not valid JSON");
    assert!(json.is_object());
    let profiles = json["claude"]["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0]["name"], "work");
    assert_eq!(profiles[0]["auth"], "api_key");
    assert!(json["claude"]["active"].is_null());
}

#[test]
fn list_active_profile_marked_in_output() {
    let env = TestEnv::new();
    add_claude(&env, "work");
    env.cmd().args(["use", "claude", "work"]).assert().success();

    env.cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(contains("work"))
        .stdout(contains("active"));
}

#[test]
fn list_json_active_field_set_after_use() {
    let env = TestEnv::new();
    add_claude(&env, "work");
    env.cmd().args(["use", "claude", "work"]).assert().success();

    let output = env
        .cmd()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["claude"]["active"], "work");
}

#[test]
fn list_invalid_tool_exits_nonzero() {
    TestEnv::new()
        .cmd()
        .args(["list", "chatgpt"])
        .assert()
        .failure();
}
