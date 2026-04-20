mod common;

use common::TestEnv;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";
const VALID_GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

fn run_json(env: &TestEnv, args: &[&str]) -> serde_json::Value {
    let output = env.output(args);
    assert!(
        output.status.success(),
        "command failed: aisw {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr should be empty in --json mode for successful command: aisw {}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be valid json")
}

fn setup_three_active_profiles(env: &TestEnv) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.add_fake_tool("codex", "codex 1.0.0");
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd().args(["init", "--yes"]).assert().success();

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success();

    env.cmd().args(["use", "claude", "work"]).assert().success();
    env.cmd().args(["use", "codex", "work"]).assert().success();
    env.cmd().args(["use", "gemini", "work"]).assert().success();
}

#[test]
fn list_json_contract_snapshot() {
    let env = TestEnv::new();
    setup_three_active_profiles(&env);

    let json = run_json(&env, &["list", "--json"]);
    let expected = serde_json::json!({
        "claude": {
            "active": "work",
            "profiles": [{"name": "work", "auth": "api_key", "label": null}],
        },
        "codex": {
            "active": "work",
            "profiles": [{"name": "work", "auth": "api_key", "label": null}],
        },
        "gemini": {
            "active": "work",
            "profiles": [{"name": "work", "auth": "api_key", "label": null}],
        }
    });

    assert_eq!(json, expected);
}

#[test]
fn status_json_contract_snapshot() {
    let env = TestEnv::new();
    setup_three_active_profiles(&env);

    let json = run_json(&env, &["status", "--json"]);
    let expected_claude_active_applied = if cfg!(target_os = "macos") {
        serde_json::Value::Null
    } else {
        serde_json::Value::Bool(true)
    };

    let expected = serde_json::json!([
        {
            "tool": "claude",
            "binary_found": true,
            "stored_profiles": 1,
            "active_profile": "work",
            "auth_method": "api_key",
            "credential_backend": "file",
            "state_mode": "isolated",
            "active_profile_applied": expected_claude_active_applied,
            "credentials_present": true,
            "permissions_ok": true,
        },
        {
            "tool": "codex",
            "binary_found": true,
            "stored_profiles": 1,
            "active_profile": "work",
            "auth_method": "api_key",
            "credential_backend": "file",
            "state_mode": "isolated",
            "active_profile_applied": true,
            "credentials_present": true,
            "permissions_ok": true,
        },
        {
            "tool": "gemini",
            "binary_found": true,
            "stored_profiles": 1,
            "active_profile": "work",
            "auth_method": "api_key",
            "credential_backend": "file",
            "state_mode": null,
            "active_profile_applied": true,
            "credentials_present": true,
            "permissions_ok": true,
        }
    ]);

    assert_eq!(json, expected);
}

#[test]
fn backup_list_json_contract_snapshot_with_normalized_ids() {
    let env = TestEnv::new();
    setup_three_active_profiles(&env);

    let mut json = run_json(&env, &["backup", "list", "--json"]);
    let entries = json
        .as_array_mut()
        .expect("backup list should return an array");
    assert!(!entries.is_empty(), "backup list should not be empty");

    for entry in entries.iter_mut() {
        let tool = entry["tool"].as_str().unwrap();
        let profile = entry["profile"].as_str().unwrap();
        assert!(matches!(tool, "claude" | "codex" | "gemini"));
        assert_eq!(profile, "work");
        assert!(entry["backup_id"].as_str().is_some());
        entry["backup_id"] = serde_json::Value::String("<backup_id>".to_owned());
    }

    let expected = serde_json::json!([
        {"backup_id": "<backup_id>", "tool": "gemini", "profile": "work"},
        {"backup_id": "<backup_id>", "tool": "codex", "profile": "work"},
        {"backup_id": "<backup_id>", "tool": "claude", "profile": "work"}
    ]);

    assert_eq!(json, expected);
}

#[test]
fn list_json_contract_preserved_with_filter_and_sort_flags() {
    let env = TestEnv::new();
    setup_three_active_profiles(&env);

    env.cmd()
        .args([
            "add",
            "claude",
            "personal",
            "--api-key",
            VALID_CLAUDE_KEY_ALT,
            "--label",
            "Personal",
        ])
        .assert()
        .success();

    let json = run_json(
        &env,
        &[
            "list",
            "--json",
            "--tool",
            "claude",
            "--search",
            "work",
            "--active-only",
            "--sort",
            "recent",
        ],
    );

    let root = json.as_object().expect("list json should be object");
    assert_eq!(root.len(), 3);
    for tool in ["codex", "gemini"] {
        let entry = root.get(tool).expect("tool key should exist");
        assert_eq!(entry["active"], serde_json::Value::Null);
        assert_eq!(
            entry["profiles"]
                .as_array()
                .expect("profiles should be array")
                .len(),
            0
        );
    }
    let claude = root.get("claude").expect("claude key should exist");
    let profiles = claude["profiles"]
        .as_array()
        .expect("profiles should be array");
    assert_eq!(profiles.len(), 1);
    let profile = profiles[0]
        .as_object()
        .expect("profile entry should be object");
    assert!(profile.contains_key("name"));
    assert!(profile.contains_key("auth"));
    assert!(profile.contains_key("label"));
    assert_eq!(profile.len(), 3);
}

#[test]
fn status_json_contract_preserved_with_filter_and_sort_flags() {
    let env = TestEnv::new();
    setup_three_active_profiles(&env);

    let json = run_json(
        &env,
        &[
            "status",
            "--json",
            "--tool",
            "claude",
            "--search",
            "work",
            "--active-only",
            "--sort",
            "recent",
        ],
    );

    let statuses = json.as_array().expect("status json should be array");
    assert_eq!(statuses.len(), 1);
    let entry = statuses[0]
        .as_object()
        .expect("status entry should be object");
    let expected_keys = [
        "tool",
        "binary_found",
        "stored_profiles",
        "active_profile",
        "auth_method",
        "credential_backend",
        "state_mode",
        "active_profile_applied",
        "credentials_present",
        "permissions_ok",
    ];
    for key in expected_keys {
        assert!(
            entry.contains_key(key),
            "missing key `{key}` in status entry"
        );
    }
    assert_eq!(entry.len(), 10);
}

#[test]
fn backup_list_json_contract_preserved_with_filter_and_sort_flags() {
    let env = TestEnv::new();
    setup_three_active_profiles(&env);

    let json = run_json(
        &env,
        &[
            "backup",
            "list",
            "--json",
            "--tool",
            "claude",
            "--search",
            "work",
            "--active-only",
            "--sort",
            "recent",
        ],
    );

    let entries = json.as_array().expect("backup list json should be array");
    assert!(
        !entries.is_empty(),
        "backup list should contain at least one entry"
    );
    for entry in entries {
        let obj = entry
            .as_object()
            .expect("backup list entry should be object");
        assert!(obj.contains_key("backup_id"));
        assert!(obj.contains_key("tool"));
        assert!(obj.contains_key("profile"));
        assert_eq!(obj.len(), 3);
    }
}
