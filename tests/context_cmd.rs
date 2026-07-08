mod common;

use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

fn setup_profiles(env: &TestEnv) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.add_fake_tool("codex", "codex 1.0.0");
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args([
            "add",
            "claude",
            "acme-claude",
            "--api-key",
            VALID_CLAUDE_KEY,
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "codex",
            "acme-codex",
            "--api-key",
            "sk-codex-test-key-12345",
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "gemini",
            "acme-gemini",
            "--api-key",
            "AIzatest1234567890ABCDEF",
        ])
        .assert()
        .success();
}

#[test]
fn context_create_and_list_json_work() {
    let env = TestEnv::new();
    setup_profiles(&env);

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

    let output = env.output(&["context", "list", "--json"]);
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let contexts = json["contexts"].as_array().unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0]["name"], "work");
    assert_eq!(contexts[0]["profiles"]["claude"], "acme-claude");
    assert_eq!(contexts[0]["profiles"]["codex"], "acme-codex");
    assert_eq!(contexts[0]["profiles"]["gemini"], serde_json::Value::Null);
}

#[test]
fn context_set_and_unset_update_only_selected_mappings() {
    let env = TestEnv::new();
    setup_profiles(&env);

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
        .args(["context", "set", "work", "--gemini", "acme-gemini"])
        .assert()
        .success();

    env.cmd()
        .args(["context", "unset", "work", "--codex"])
        .assert()
        .success();

    let output = env.output(&["context", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let profiles = &json["contexts"][0]["profiles"];
    assert_eq!(profiles["claude"], "acme-claude");
    assert_eq!(profiles["codex"], serde_json::Value::Null);
    assert_eq!(profiles["gemini"], "acme-gemini");
}

#[test]
fn context_unset_rejects_empty_context() {
    let env = TestEnv::new();
    setup_profiles(&env);

    env.cmd()
        .args(["context", "create", "work", "--claude", "acme-claude"])
        .assert()
        .success();

    env.cmd()
        .args(["context", "unset", "work", "--claude"])
        .assert()
        .failure();
}

#[test]
fn context_rename_and_remove_work() {
    let env = TestEnv::new();
    setup_profiles(&env);

    env.cmd()
        .args(["context", "create", "work", "--claude", "acme-claude"])
        .assert()
        .success();

    env.cmd()
        .args(["context", "rename", "work", "client-acme"])
        .assert()
        .success();
    env.cmd()
        .args(["context", "remove", "client-acme", "--yes"])
        .assert()
        .success();

    let output = env.output(&["context", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["contexts"].as_array().unwrap().len(), 0);
}

#[test]
fn profile_remove_is_blocked_when_context_references_it() {
    let env = TestEnv::new();
    setup_profiles(&env);

    env.cmd()
        .args(["context", "create", "work", "--codex", "acme-codex"])
        .assert()
        .success();

    env.cmd()
        .args(["remove", "codex", "acme-codex", "--yes", "--force"])
        .assert()
        .failure();
}

#[test]
fn context_use_emit_env_combines_exports_and_updates_active_profiles() {
    let env = TestEnv::new();
    setup_profiles(&env);

    env.cmd()
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "acme-claude",
            "--codex",
            "acme-codex",
            "--gemini",
            "acme-gemini",
        ])
        .assert()
        .success();

    env.cmd()
        .args(["context", "use", "work", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("CLAUDE_CONFIG_DIR"))
        .stdout(contains("CODEX_HOME"))
        .stdout(contains("GEMINI_API_KEY"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "acme-claude");
    assert_eq!(config["active"]["codex"], "acme-codex");
    assert_eq!(config["active"]["gemini"], "acme-gemini");
}

#[test]
fn context_use_shared_emit_env_applies_shared_mode_only_to_supported_tools() {
    let env = TestEnv::new();
    setup_profiles(&env);

    env.cmd()
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "acme-claude",
            "--codex",
            "acme-codex",
            "--gemini",
            "acme-gemini",
        ])
        .assert()
        .success();

    env.cmd()
        .args([
            "context",
            "use",
            "work",
            "--state-mode",
            "shared",
            "--emit-env",
        ])
        .assert()
        .success()
        .stdout(contains("unset CLAUDE_CONFIG_DIR"))
        .stdout(contains("unset CODEX_HOME"))
        .stdout(contains("GEMINI_API_KEY"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["settings"]["claude"]["state_mode"], "shared");
    assert_eq!(config["settings"]["codex"]["state_mode"], "shared");
}

#[test]
fn context_use_writes_live_files_and_updates_active_profiles() {
    let env = TestEnv::new();
    setup_profiles(&env);

    env.cmd()
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "acme-claude",
            "--codex",
            "acme-codex",
            "--gemini",
            "acme-gemini",
        ])
        .assert()
        .success();

    env.cmd()
        .args(["context", "use", "work"])
        .assert()
        .success();

    assert!(env
        .fake_home
        .join(".claude")
        .join(".credentials.json")
        .exists());
    assert!(env.fake_home.join(".codex").join("auth.json").exists());
    assert!(env.fake_home.join(".codex").join("config.toml").exists());
    assert!(env.fake_home.join(".gemini").join(".env").exists());

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "acme-claude");
    assert_eq!(config["active"]["codex"], "acme-codex");
    assert_eq!(config["active"]["gemini"], "acme-gemini");
}

#[test]
fn context_use_json_reports_machine_activation_state() {
    let env = TestEnv::new();
    setup_profiles(&env);

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

    let output = env.output(&["context", "use", "work", "--json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "context_use");
    assert_eq!(json["result"]["context"], "work");
    assert_eq!(json["result"]["active"]["claude"], "acme-claude");
    assert_eq!(json["result"]["active"]["codex"], "acme-codex");
    assert_eq!(json["result"]["active"]["gemini"], serde_json::Value::Null);
}

#[test]
fn context_rename_json_reports_new_context_state() {
    let env = TestEnv::new();
    setup_profiles(&env);

    env.cmd()
        .args(["context", "create", "work", "--claude", "acme-claude"])
        .assert()
        .success();

    let output = env.output(&["context", "rename", "work", "client-acme", "--json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "context_rename");
    assert_eq!(json["result"]["old_name"], "work");
    assert_eq!(json["result"]["new_name"], "client-acme");
    assert_eq!(json["result"]["context"]["name"], "client-acme");
}

#[test]
fn failed_context_use_rolls_back_live_state_and_active_profiles() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args([
            "add",
            "claude",
            "personal",
            "--api-key",
            VALID_CLAUDE_KEY_ALT,
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "claude",
            "acme-claude",
            "--api-key",
            VALID_CLAUDE_KEY,
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "codex",
            "acme-codex",
            "--api-key",
            "sk-codex-test-key-12345",
        ])
        .assert()
        .success();

    env.cmd()
        .args(["use", "claude", "personal"])
        .assert()
        .success();

    let live_before =
        std::fs::read(env.fake_home.join(".claude").join(".credentials.json")).unwrap();

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
        .env("AISW_FAULT_INJECTION", "live_apply.commit_write:2")
        .args(["context", "use", "work"])
        .assert()
        .failure();

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "personal");
    assert_eq!(config["active"]["codex"], serde_json::Value::Null);

    let live_after =
        std::fs::read(env.fake_home.join(".claude").join(".credentials.json")).unwrap();
    assert_eq!(live_after, live_before);
    assert!(
        !env.fake_home.join(".codex").join("auth.json").exists(),
        "codex auth should be rolled back on failure"
    );
}
