// Integration tests for `aisw use`.
mod common;

use common::assert_output_redacts_secret;
use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";
const VALID_CODEX_KEY_ALT: &str = "sk-codex-test-key-67890";
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

fn add_codex_profile(env: &TestEnv, name: &str) {
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "codex", name, "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();
}

fn write_config_json(env: &TestEnv, json: serde_json::Value) {
    std::fs::write(
        env.aisw_home.join("config.json"),
        serde_json::to_string_pretty(&json).unwrap(),
    )
    .unwrap();
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
        .stdout(contains("export CLAUDE_CONFIG_DIR='"));
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
        .stdout(contains("export ANTHROPIC_API_KEY='"));
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
        .stdout(contains("export GEMINI_API_KEY='"));
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
fn use_codex_oauth_emit_env_quotes_path_with_shell_chars() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    let profile_dir = env.aisw_home.join("profiles").join("codex").join("work");
    std::fs::create_dir_all(&profile_dir).unwrap();
    std::fs::write(profile_dir.join("auth.json"), r#"{"token":"tok"}"#).unwrap();
    std::fs::write(
        profile_dir.join("config.toml"),
        "cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();
    let config_json = serde_json::json!({
        "version": 1,
        "active": {"claude": null, "codex": null, "gemini": null},
        "profiles": {
            "claude": {},
            "codex": {
                "work": {
                    "added_at": "2026-03-25T00:00:00Z",
                    "auth_method": "o_auth",
                    "label": null
                }
            },
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
        .args(["use", "codex", "work", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("export CODEX_HOME='"))
        .stdout(contains("/profiles/codex/work'"));
}

#[test]
fn use_codex_shared_emit_env_unsets_codex_home() {
    let env = TestEnv::new();
    add_codex_profile(&env, "work");

    env.cmd()
        .args([
            "use",
            "codex",
            "work",
            "--state-mode",
            "shared",
            "--emit-env",
        ])
        .assert()
        .success()
        .stdout(contains("unset CODEX_HOME"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["settings"]["codex"]["state_mode"], "shared");
}

#[test]
fn use_without_emit_env_prints_switched_message() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    env.cmd()
        .args(["use", "claude", "work"])
        .assert()
        .success()
        .stdout(contains("Switched profile"))
        .stdout(contains("Tool"))
        .stdout(contains("Claude Code"))
        .stdout(contains("Active profile"))
        .stdout(contains("work"))
        .stdout(contains("Auth"))
        .stdout(contains("api_key"))
        .stdout(contains("Next"))
        .stdout(contains("aisw status"));

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
        .stdout(contains("Switched profile"))
        .stdout(contains("Tool"))
        .stdout(contains("Claude Code"))
        .stdout(contains("Active profile"))
        .stdout(contains("work"))
        .stdout(contains("Auth"))
        .stdout(contains("api_key"))
        .stdout(contains("Next"))
        .stdout(contains("aisw status"));
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
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();

    env.cmd().args(["use", "codex", "work"]).assert().success();

    assert!(env.fake_home.join(".codex").join("auth.json").exists());
    let config = std::fs::read_to_string(env.fake_home.join(".codex").join("config.toml")).unwrap();
    assert!(config.contains("model = \"gpt-5.4\""));
    assert!(config.contains("cli_auth_credentials_store = \"file\""));
}

#[test]
fn use_codex_shared_mode_preserves_existing_shared_session_files() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");
    let sessions_dir = env
        .fake_home
        .join(".codex")
        .join("sessions")
        .join("2026")
        .join("03")
        .join("30");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let sentinel = sessions_dir.join("existing-session.jsonl");
    std::fs::write(&sentinel, "session").unwrap();

    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();

    env.cmd()
        .args(["use", "codex", "work", "--state-mode", "shared"])
        .assert()
        .success()
        .stdout(contains("State mode"))
        .stdout(contains("shared"));

    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "session");
}

#[test]
fn use_state_mode_is_rejected_for_non_codex_tools() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    env.cmd()
        .args(["use", "claude", "work", "--state-mode", "shared"])
        .assert()
        .failure()
        .stderr(contains("currently supported only for codex"));
}

#[test]
fn failed_codex_switch_does_not_advance_active_profile_when_first_write_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args(["add", "codex", "old", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "codex", "new", "--api-key", VALID_CODEX_KEY_ALT])
        .assert()
        .success();
    env.cmd().args(["use", "codex", "old"]).assert().success();

    let auth_path = env.fake_home.join(".codex").join("auth.json");
    let config_path = env.fake_home.join(".codex").join("config.toml");
    let auth_before = std::fs::read(&auth_path).unwrap();
    let config_before = std::fs::read(&config_path).unwrap();

    env.cmd()
        .env("AISW_FAULT_INJECTION", "live_apply.commit_write:1")
        .args(["use", "codex", "new"])
        .assert()
        .failure()
        .stderr(contains("injected live-apply failure"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["codex"], "old");
    assert_eq!(std::fs::read(&auth_path).unwrap(), auth_before);
    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
}

#[test]
fn failed_codex_switch_rolls_back_partial_live_writes() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args(["add", "codex", "old", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "codex", "new", "--api-key", VALID_CODEX_KEY_ALT])
        .assert()
        .success();
    env.cmd().args(["use", "codex", "old"]).assert().success();

    let auth_path = env.fake_home.join(".codex").join("auth.json");
    let config_path = env.fake_home.join(".codex").join("config.toml");
    let auth_before = std::fs::read(&auth_path).unwrap();
    let config_before = std::fs::read(&config_path).unwrap();

    env.cmd()
        .env("AISW_FAULT_INJECTION", "live_apply.commit_write:2")
        .args(["use", "codex", "new"])
        .assert()
        .failure()
        .stderr(contains("injected live-apply failure"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["codex"], "old");
    assert_eq!(std::fs::read(&auth_path).unwrap(), auth_before);
    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
}

#[test]
fn failed_claude_switch_does_not_advance_active_profile_or_live_credentials() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "old", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "claude", "new", "--api-key", VALID_CLAUDE_KEY_ALT])
        .assert()
        .success();
    env.cmd().args(["use", "claude", "old"]).assert().success();

    let live_path = env.fake_home.join(".claude").join(".credentials.json");
    let live_before = std::fs::read(&live_path).unwrap();

    env.cmd()
        .env("AISW_FAULT_INJECTION", "live_apply.commit_write:1")
        .args(["use", "claude", "new"])
        .assert()
        .failure()
        .stderr(contains("injected live-apply failure"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "old");
    assert_eq!(std::fs::read(&live_path).unwrap(), live_before);
}

#[test]
fn failed_gemini_oauth_switch_rolls_back_partial_live_writes() {
    let env = TestEnv::new();

    let old_dir = env.aisw_home.join("profiles").join("gemini").join("old");
    let new_dir = env.aisw_home.join("profiles").join("gemini").join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();
    std::fs::write(old_dir.join("oauth_creds.json"), r#"{"token":"old"}"#).unwrap();
    std::fs::write(old_dir.join("state.json"), r#"{"account":"old"}"#).unwrap();
    std::fs::write(new_dir.join("oauth_creds.json"), r#"{"token":"new"}"#).unwrap();
    std::fs::write(new_dir.join("state.json"), r#"{"account":"new"}"#).unwrap();

    write_config_json(
        &env,
        serde_json::json!({
            "version": 1,
            "active": {"claude": null, "codex": null, "gemini": null},
            "profiles": {
                "claude": {},
                "codex": {},
                "gemini": {
                    "old": {
                        "added_at": "2026-03-25T00:00:00Z",
                        "auth_method": "o_auth",
                        "label": null
                    },
                    "new": {
                        "added_at": "2026-03-25T00:00:00Z",
                        "auth_method": "o_auth",
                        "label": null
                    }
                }
            },
            "settings": {"backup_on_switch": true, "max_backups": 10}
        }),
    );

    env.cmd().args(["use", "gemini", "old"]).assert().success();

    let gemini_dir = env.fake_home.join(".gemini");
    let oauth_before = std::fs::read(gemini_dir.join("oauth_creds.json")).unwrap();
    let state_before = std::fs::read(gemini_dir.join("state.json")).unwrap();

    env.cmd()
        .env("AISW_FAULT_INJECTION", "live_apply.commit_write:2")
        .args(["use", "gemini", "new"])
        .assert()
        .failure()
        .stderr(contains("injected live-apply failure"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["gemini"], "old");
    assert_eq!(
        std::fs::read(gemini_dir.join("oauth_creds.json")).unwrap(),
        oauth_before
    );
    assert_eq!(
        std::fs::read(gemini_dir.join("state.json")).unwrap(),
        state_before
    );
}

#[test]
fn use_quiet_suppresses_human_summary_output() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    let output = env.output(&["--quiet", "use", "claude", "work"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "expected quiet use to be silent: {stdout}"
    );
}

#[test]
fn failing_claude_use_does_not_leak_api_key() {
    let env = TestEnv::new();
    add_claude_profile(&env, "work");

    let live_dir = env.fake_home.join(".claude");
    std::fs::create_dir_all(&live_dir).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&live_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    let output = env.output(&["use", "claude", "work"]);
    assert!(!output.status.success(), "use should fail");
    assert_output_redacts_secret(&output, VALID_CLAUDE_KEY);
}

#[test]
fn failing_codex_use_does_not_leak_api_key() {
    let env = TestEnv::new();
    add_codex_profile(&env, "work");

    let live_dir = env.fake_home.join(".codex");
    std::fs::create_dir_all(&live_dir).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&live_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    let output = env.output(&["use", "codex", "work"]);
    assert!(!output.status.success(), "use should fail");
    assert_output_redacts_secret(&output, VALID_CODEX_KEY);
}

#[test]
fn failing_gemini_use_does_not_leak_api_key() {
    let env = TestEnv::new();
    add_gemini_profile(&env, "work");

    let live_dir = env.fake_home.join(".gemini");
    std::fs::create_dir_all(&live_dir).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&live_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    let output = env.output(&["use", "gemini", "work"]);
    assert!(!output.status.success(), "use should fail");
    assert_output_redacts_secret(&output, VALID_GEMINI_KEY);
}
