// Integration tests for `aisw add` across all tools.
mod common;

use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";
const VALID_GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

// ---- Claude ----

#[test]
fn add_claude_api_key_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("Tool"))
        .stdout(contains("Claude Code"))
        .stdout(contains("work"))
        .stdout(contains("Next"))
        .stdout(contains("aisw use claude work"));
}

#[test]
fn add_claude_tool_not_installed_fails() {
    // No claude binary added to PATH.
    TestEnv::new()
        .cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .failure()
        .stderr(contains("not installed"));
}

#[test]
fn add_claude_api_key_with_set_active() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            VALID_CLAUDE_KEY,
            "--set-active",
        ])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "work");
}

#[test]
fn add_claude_api_key_with_label() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            VALID_CLAUDE_KEY,
            "--label",
            "My work account",
        ])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["work"]["label"],
        "My work account"
    );
}

#[test]
fn add_invalid_profile_name_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    // Space in profile name is invalid.
    env.cmd()
        .args(["add", "claude", "my profile", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .failure();
}

#[test]
fn add_duplicate_profile_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

// ---- Codex ----

#[test]
fn add_codex_api_key_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("Tool"))
        .stdout(contains("Codex CLI"))
        .stdout(contains("work"))
        .stdout(contains("Next"))
        .stdout(contains("aisw use codex work"));
}

#[test]
fn add_codex_tool_not_installed_fails() {
    TestEnv::new()
        .cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .failure()
        .stderr(contains("not installed"));
}

// ---- Gemini ----

#[test]
fn add_gemini_api_key_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("Tool"))
        .stdout(contains("Gemini CLI"))
        .stdout(contains("work"))
        .stdout(contains("Next"))
        .stdout(contains("aisw use gemini work"));
}

#[test]
fn add_gemini_tool_not_installed_fails() {
    TestEnv::new()
        .cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .failure()
        .stderr(contains("not installed"));
}
