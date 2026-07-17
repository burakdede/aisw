// Integration tests for `aisw status`.
mod common;

use std::os::unix::fs::PermissionsExt;

use common::TestEnv;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";
const VALID_GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";
const ANTIGRAVITY_SECRET: &str = r#"{"email":"work@example.com","token":"work-live"}"#;

fn add_and_activate_claude(env: &TestEnv, name: &str) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", name, "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();
    env.cmd().args(["use", "claude", name]).assert().success();
}

fn add_and_activate_codex(env: &TestEnv, name: &str) {
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "codex", name, "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();
    env.cmd().args(["use", "codex", name]).assert().success();
}

fn add_and_activate_gemini(env: &TestEnv, name: &str) {
    env.add_fake_tool("gemini", "gemini 0.9.0");
    env.cmd()
        .args(["add", "gemini", name, "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success();
    env.cmd().args(["use", "gemini", name]).assert().success();
}

fn add_claude_profile(env: &TestEnv, name: &str, key: &str) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args(["add", "claude", name, "--api-key", key])
        .assert()
        .success();
}

fn add_codex_profile(env: &TestEnv, name: &str, key: &str) {
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "codex", name, "--api-key", key])
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

fn add_and_activate_antigravity(env: &TestEnv, name: &str) {
    env.add_fake_tool("agy", "agy 1.0.0");
    write_antigravity_live_state(env, ANTIGRAVITY_SECRET);
    env.cmd()
        .args(["add", "antigravity", name, "--from-live"])
        .assert()
        .success();
    env.cmd()
        .args(["use", "antigravity", name])
        .assert()
        .success();
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
        .stdout(contains("Status"))
        .stdout(contains("Claude Code"))
        .stdout(contains("work"))
        .stdout(contains("State"))
        .stdout(contains("credentials present"));
}

#[test]
fn status_reports_missing_system_keyring_credentials_explicitly() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    std::fs::create_dir_all(env.aisw_home.join("profiles").join("claude").join("work")).unwrap();

    let config_json = serde_json::json!({
        "version": 1,
        "active": {"claude": "work", "codex": null, "gemini": null},
        "profiles": {
            "claude": {
                "work": {
                    "added_at": "2026-03-30T00:00:00Z",
                    "auth_method": "o_auth",
                    "credential_backend": "system_keyring",
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
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("Backend"))
        .stdout(contains("system_keyring"))
        .stdout(contains(
            "secure credentials missing from the managed system keyring",
        ));
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
    assert_eq!(arr.len(), 4); // one entry per tool

    let claude = arr.iter().find(|e| e["tool"] == "claude").unwrap();
    assert_eq!(claude["binary_found"], true);
    assert_eq!(claude["stored_profiles"], 1);
    assert_eq!(claude["active_profile"], "work");
    assert_eq!(claude["state_mode"], "isolated");
    assert_eq!(claude["claude_auth_classification"], "api_key");
    if cfg!(target_os = "macos") {
        assert!(claude["active_profile_applied"].is_null());
    } else {
        assert_eq!(claude["active_profile_applied"], true);
    }
    assert_eq!(claude["credentials_present"], true);
    assert_eq!(claude["permissions_ok"], true);
}

#[test]
fn status_json_reports_antigravity_classification_and_live_state() {
    let env = TestEnv::new();
    add_and_activate_antigravity(&env, "work");

    let output = env
        .cmd()
        .args(["status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).expect("invalid JSON");
    let arr = json.as_array().unwrap();
    let antigravity = arr.iter().find(|e| e["tool"] == "agy").unwrap();
    assert_eq!(antigravity["binary_found"], true);
    assert_eq!(antigravity["stored_profiles"], 1);
    assert_eq!(antigravity["active_profile"], "work");
    assert_eq!(antigravity["auth_method"], "oauth");
    assert_eq!(antigravity["credential_backend"], "file");
    assert_eq!(
        antigravity["antigravity_auth_classification"],
        "oauth_shared_live_keyring"
    );
    assert!(antigravity["state_mode"].is_null());
    assert_eq!(antigravity["active_profile_applied"], true);
    assert_eq!(antigravity["credentials_present"], true);
    assert_eq!(antigravity["permissions_ok"], true);
}

#[test]
fn status_context_json_wraps_tools_and_context_summary() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "acme-claude");
    add_and_activate_codex(&env, "acme-codex");

    env.cmd()
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "acme-claude",
            "--codex",
            "acme-codex",
        ])
        .assert()
        .success();

    let output = env
        .cmd()
        .args(["status", "--context", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).expect("invalid JSON");
    assert!(json["tools"].is_array());
    assert_eq!(json["context"]["status"], "exact");
    assert_eq!(json["context"]["active"], "work");
    assert_eq!(json["context"]["profiles"]["claude"], "acme-claude");
    assert_eq!(json["context"]["profiles"]["codex"], "acme-codex");
}

#[test]
fn status_context_reports_ambiguous_matches() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "acme-claude");
    add_and_activate_codex(&env, "acme-codex");

    env.cmd()
        .args([
            "context",
            "create",
            "claude-only",
            "--claude",
            "acme-claude",
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "acme-claude",
            "--codex",
            "acme-codex",
        ])
        .assert()
        .success();

    env.cmd()
        .args(["status", "--context"])
        .assert()
        .success()
        .stdout(contains("Context"))
        .stdout(contains("ambiguous"))
        .stdout(contains("claude-only, work"));
}

#[test]
fn status_context_reports_unmanaged_tools_for_sparse_exact_match() {
    let env = TestEnv::new();
    add_claude_profile(
        &env,
        "acme-claude",
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    );
    add_codex_profile(&env, "acme-codex", VALID_CODEX_KEY);
    env.cmd()
        .args(["use", "claude", "acme-claude"])
        .assert()
        .success();
    env.cmd()
        .args(["use", "codex", "acme-codex"])
        .assert()
        .success();

    env.cmd()
        .args([
            "context",
            "create",
            "claude-only",
            "--claude",
            "acme-claude",
        ])
        .assert()
        .success();

    env.cmd()
        .args(["status", "--context"])
        .assert()
        .success()
        .stdout(contains("Active context"))
        .stdout(contains("claude-only"))
        .stdout(contains("Unmanaged"))
        .stdout(contains("codex=acme-codex"));
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
fn status_reports_live_tool_config_mismatch_for_active_claude_profile() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "work");

    std::fs::remove_file(env.fake_home.join(".claude").join(".credentials.json")).unwrap();

    let assertion = env.cmd().args(["status"]).assert().success();
    if cfg!(target_os = "macos") {
        assertion.stdout(contains("live macOS Keychain not checked"));
    } else {
        assertion.stdout(contains(
            "live tool config does not match the active profile",
        ));
    }
}

#[test]
fn status_reports_live_tool_config_mismatch_for_active_codex_profile() {
    let env = TestEnv::new();
    add_and_activate_codex(&env, "work");

    std::fs::remove_file(env.fake_home.join(".codex").join("auth.json")).unwrap();

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains(
            "live tool config does not match the active profile",
        ));
}

#[test]
fn status_reports_live_tool_config_mismatch_for_active_antigravity_profile() {
    let env = TestEnv::new();
    add_and_activate_antigravity(&env, "work");

    std::fs::write(
        env.fake_home
            .join("keychain")
            .join("gemini")
            .join("antigravity")
            .join("secret"),
        br#"{"email":"other@example.com","token":"other"}"#,
    )
    .unwrap();

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains(
            "live tool config does not match the active profile",
        ));
}

#[test]
fn status_shows_claude_state_mode() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "work");

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("Claude Code"))
        .stdout(contains("State mode"))
        .stdout(contains("isolated"));
}

#[test]
fn status_shows_codex_state_mode() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["use", "codex", "work", "--state-mode", "shared"])
        .assert()
        .success();

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("Codex CLI"))
        .stdout(contains("State mode"))
        .stdout(contains("shared"));
}

#[test]
fn status_reports_live_tool_config_mismatch_for_active_gemini_api_key_profile() {
    let env = TestEnv::new();
    add_and_activate_gemini(&env, "work");

    std::fs::remove_file(env.fake_home.join(".gemini").join(".env")).unwrap();

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains(
            "live tool config does not match the active profile",
        ));
}

#[test]
fn status_reports_live_tool_config_mismatch_for_active_gemini_oauth_profile() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    let profile_dir = env.aisw_home.join("profiles").join("gemini").join("work");
    std::fs::create_dir_all(&profile_dir).unwrap();
    std::fs::write(profile_dir.join("oauth_creds.json"), r#"{"token":"tok"}"#).unwrap();
    std::fs::write(profile_dir.join("settings.json"), r#"{"account":"work"}"#).unwrap();
    std::fs::set_permissions(
        profile_dir.join("oauth_creds.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    std::fs::set_permissions(
        profile_dir.join("settings.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    std::fs::create_dir_all(env.fake_home.join(".gemini")).unwrap();
    std::fs::write(
        env.fake_home.join(".gemini").join("oauth_creds.json"),
        r#"{"token":"different"}"#,
    )
    .unwrap();
    std::fs::write(
        env.fake_home.join(".gemini").join("settings.json"),
        r#"{"account":"different"}"#,
    )
    .unwrap();

    let config_json = serde_json::json!({
        "version": 1,
        "active": {"claude": null, "codex": null, "gemini": "work"},
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
        .args(["status"])
        .assert()
        .success()
        .stdout(contains(
            "live tool config does not match the active profile",
        ));
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
        .stdout(contains("Active"))
        .stdout(contains("none"))
        .stdout(contains("profiles stored, but none is active"));
}

#[test]
fn status_supports_tool_search_and_active_only_filters() {
    let env = TestEnv::new();
    add_and_activate_claude(&env, "work");
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "codex", "main", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();

    env.cmd()
        .args([
            "status",
            "--tool",
            "claude",
            "--search",
            "work",
            "--active-only",
        ])
        .assert()
        .success()
        .stdout(contains("Claude Code"))
        .stdout(contains("work"))
        .stdout(predicates::str::contains("Codex CLI").not());
}
