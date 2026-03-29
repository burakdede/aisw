// Integration tests for `aisw add` across all tools.
mod common;

use common::assert_output_redacts_secret;
use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";
const VALID_CODEX_KEY_ALT: &str = "sk-codex-test-key-67890";
const VALID_GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";
const VALID_GEMINI_KEY_ALT: &str = "AIzaalt0987654321FEDCBA";

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

#[test]
fn add_duplicate_claude_api_key_under_different_name_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();

    let output = env.output(&["add", "claude", "backup", "--api-key", VALID_CLAUDE_KEY]);
    assert!(!output.status.success(), "duplicate add should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("API key already exists as profile 'work'"));
    assert_output_redacts_secret(&output, VALID_CLAUDE_KEY);
}

#[test]
fn add_distinct_claude_api_keys_under_different_names_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();

    env.cmd()
        .args([
            "add",
            "claude",
            "personal",
            "--api-key",
            VALID_CLAUDE_KEY_ALT,
        ])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("personal"));
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

#[test]
fn add_duplicate_codex_api_key_under_different_name_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();

    let output = env.output(&["add", "codex", "backup", "--api-key", VALID_CODEX_KEY]);
    assert!(!output.status.success(), "duplicate add should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("API key already exists as profile 'work'"));
    assert_output_redacts_secret(&output, VALID_CODEX_KEY);
}

#[test]
fn add_distinct_codex_api_keys_under_different_names_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();

    env.cmd()
        .args(["add", "codex", "personal", "--api-key", VALID_CODEX_KEY_ALT])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("personal"));
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

#[test]
fn add_duplicate_gemini_api_key_under_different_name_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success();

    let output = env.output(&["add", "gemini", "backup", "--api-key", VALID_GEMINI_KEY]);
    assert!(!output.status.success(), "duplicate add should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("API key already exists as profile 'work'"));
    assert_output_redacts_secret(&output, VALID_GEMINI_KEY);
}

#[test]
fn add_distinct_gemini_api_keys_under_different_names_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success();

    env.cmd()
        .args([
            "add",
            "gemini",
            "personal",
            "--api-key",
            VALID_GEMINI_KEY_ALT,
        ])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("personal"));
}
