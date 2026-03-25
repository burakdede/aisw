// Integration tests for `aisw use`.
mod common;

use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

fn add_claude_profile(env: &TestEnv, name: &str) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", name, "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();
}

fn add_gemini_profile(env: &TestEnv, name: &str) {
    env.add_fake_tool("gemini", "gemini 0.9.0");
    env.cmd()
        .args(["add", "gemini", name, "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success();
}

#[test]
fn use_claude_oauth_emit_env_prints_claude_config_dir() {
    let env = TestEnv::new();
    // Pre-populate an OAuth profile without going through the interactive flow.
    let profile_dir = env.aisw_home.join("profiles").join("claude").join("work");
    std::fs::create_dir_all(&profile_dir).unwrap();
    std::fs::write(
        profile_dir.join(".credentials.json"),
        r#"{"oauthToken":"tok"}"#,
    )
    .unwrap();
    let config_json = serde_json::json!({
        "version": 1,
        "active": {"claude": null, "codex": null, "gemini": null},
        "profiles": {
            "claude": {
                "work": {
                    "added_at": "2026-03-25T00:00:00Z",
                    "auth_method": "o_auth",
                    "label": null
                }
            },
            "codex": {},
            "gemini": {}
        },
        "settings": {"backup_on_switch": true, "max_backups": 10}
    });
    std::fs::write(
        env.aisw_home.join("config.json"),
        serde_json::to_string_pretty(&config_json).unwrap(),
    )
    .unwrap();

    env.cmd()
        .args(["use", "claude", "work", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("export CLAUDE_CONFIG_DIR="));
}

#[test]
fn use_claude_emit_env_anthropic_api_key_for_api_key_profile() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    // API key profile → should emit ANTHROPIC_API_KEY
    env.cmd()
        .args(["use", "claude", "work", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("export ANTHROPIC_API_KEY="));
}

#[test]
fn use_nonexistent_profile_fails() {
    TestEnv::new()
        .cmd()
        .args(["use", "claude", "ghost", "--emit-env"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

#[test]
fn use_updates_active_profile_in_config() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    env.cmd()
        .args(["use", "claude", "work", "--emit-env"])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "work");
}

#[test]
fn use_creates_backup_in_backups_dir() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    env.cmd()
        .args(["use", "claude", "work", "--emit-env"])
        .assert()
        .success();

    // backups/ should have been created with at least one entry.
    let backups_dir = env.home_file("backups");
    assert!(
        backups_dir.exists(),
        "backups dir should exist after switch"
    );
    let entries: Vec<_> = std::fs::read_dir(&backups_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "at least one backup expected");
}

#[test]
fn use_gemini_api_key_rewrites_gemini_env() {
    let env = TestEnv::new();
    add_gemini_profile(&env, "work");

    env.cmd().args(["use", "gemini", "work"]).assert().success();

    // ~/.gemini/.env (inside fake_home) should be written.
    let gemini_env = env.fake_home.join(".gemini").join(".env");
    assert!(gemini_env.exists(), "~/.gemini/.env should be written");
    let contents = std::fs::read_to_string(&gemini_env).unwrap();
    assert!(contents.contains("GEMINI_API_KEY="));
}

#[test]
fn use_gemini_api_key_emit_env_prints_gemini_key() {
    let env = TestEnv::new();
    add_gemini_profile(&env, "work");

    env.cmd()
        .args(["use", "gemini", "work", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("export GEMINI_API_KEY="));
}

#[test]
fn use_gemini_oauth_emit_env_unsets_gemini_key() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    let profile_dir = env.aisw_home.join("profiles").join("gemini").join("work");
    std::fs::create_dir_all(&profile_dir).unwrap();
    std::fs::write(profile_dir.join("oauth_creds.json"), r#"{"token":"tok"}"#).unwrap();
    let config_json = serde_json::json!({
        "version": 1,
        "active": {"claude": null, "codex": null, "gemini": null},
        "profiles": {
            "claude": {},
            "codex": {},
            "gemini": {
                "work": {
                    "added_at": "2026-03-25T00:00:00Z",
                    "auth_method": "o_auth",
                    "label": null
                }
            }
        },
        "settings": {"backup_on_switch": true, "max_backups": 10}
    });
    std::fs::write(
        env.aisw_home.join("config.json"),
        serde_json::to_string_pretty(&config_json).unwrap(),
    )
    .unwrap();

    env.cmd()
        .args(["use", "gemini", "work", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("unset GEMINI_API_KEY"));
}

#[test]
fn use_without_emit_env_prints_switched_message() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    env.cmd()
        .args(["use", "claude", "work"])
        .assert()
        .success()
        .stdout(contains("Switched claude to profile 'work'."))
        .stdout(contains(
            "Next: run 'aisw status' to confirm the current state.",
        ));

    let live = env.fake_home.join(".claude").join(".credentials.json");
    assert!(live.exists(), "live Claude credentials should be written");
}

#[test]
fn use_prints_switched_without_shell_env_matching() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    env.cmd()
        .args(["use", "claude", "work"])
        .assert()
        .success()
        .stdout(contains("Switched claude to profile 'work'."))
        .stdout(contains(
            "Next: run 'aisw status' to confirm the current state.",
        ));
}

#[test]
fn use_codex_writes_live_auth_files() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");
    std::fs::create_dir_all(env.fake_home.join(".codex")).unwrap();
    std::fs::write(
        env.fake_home.join(".codex").join("config.toml"),
        "model = \"gpt-5.4\"\n",
    )
    .unwrap();

    env.cmd()
        .args([
            "add",
            "codex",
            "work",
            "--api-key",
            "sk-codex-test-key-12345",
        ])
        .assert()
        .success();

    env.cmd().args(["use", "codex", "work"]).assert().success();

    assert!(env.fake_home.join(".codex").join("auth.json").exists());
    let config = std::fs::read_to_string(env.fake_home.join(".codex").join("config.toml")).unwrap();
    assert!(config.contains("model = \"gpt-5.4\""));
    assert!(config.contains("cli_auth_credentials_store = \"file\""));
}
