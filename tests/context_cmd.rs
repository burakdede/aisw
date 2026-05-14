mod common;

use common::TestEnv;

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
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ])
        .assert()
        .success();
    env.cmd()
        .args(["add", "codex", "acme-codex", "--api-key", "sk-codex-test-key-12345"])
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
        .args([
            "context",
            "set",
            "work",
            "--gemini",
            "acme-gemini",
        ])
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
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "acme-claude",
        ])
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
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "acme-claude",
        ])
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
        .args([
            "context",
            "create",
            "work",
            "--codex",
            "acme-codex",
        ])
        .assert()
        .success();

    env.cmd()
        .args(["remove", "codex", "acme-codex", "--yes", "--force"])
        .assert()
        .failure();
}
