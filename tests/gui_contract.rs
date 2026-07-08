mod common;

use common::TestEnv;

fn json_output(env: &TestEnv, args: &[&str]) -> serde_json::Value {
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
        "stderr should be empty for machine success: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn version_json_reports_contract_versions() {
    let env = TestEnv::new();
    let json = json_output(&env, &["version", "--json"]);
    assert!(json["version"].as_str().is_some());
    assert_eq!(json["cli_api_version"], 1);
    assert_eq!(json["json_schema_version"], 1);
    assert_eq!(json["progress_schema_version"], 1);
}

#[test]
fn capabilities_json_reports_tool_capabilities() {
    let env = TestEnv::new();
    let json = json_output(&env, &["capabilities", "--json"]);
    assert_eq!(json["features"]["api_key_stdin"], true);
    assert_eq!(json["features"]["mutation_json"], true);
    assert_eq!(json["features"]["progress_json"], true);
    assert_eq!(json["features"]["non_prompting_init"], true);
    assert_eq!(json["features"]["detect_live_init"], true);
    assert_eq!(json["tools"]["gemini"]["state_modes"][0], "isolated");
    assert_eq!(json["tools"]["codex"]["fail_closed_keyring_identity"], true);
}

#[test]
fn init_json_detect_live_returns_machine_bootstrap_state() {
    let env = TestEnv::new();
    std::fs::create_dir_all(env.fake_home.join(".claude")).unwrap();
    std::fs::write(
        env.fake_home.join(".claude").join(".credentials.json"),
        br#"{"oauthToken":"tok"}"#,
    )
    .unwrap();

    let output = env
        .cmd()
        .args(["init", "--json", "--no-shell-hook", "--detect-live"])
        .env("SHELL", "/bin/zsh")
        .env("AISW_CLAUDE_AUTH_STORAGE", "file")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "init");
    assert_eq!(json["result"]["shell"]["action"], "skipped");
    assert_eq!(json["result"]["shell"]["detected"], "zsh");
    assert_eq!(json["result"]["live_accounts"][0]["tool"], "claude");
    assert_eq!(json["result"]["live_accounts"][0]["outcome"], "detected");
    assert_eq!(json["result"]["live_accounts"][0]["auth_method"], "oauth");
    assert!(!env.fake_home.join(".zshrc").exists());
}

#[test]
fn parse_errors_are_structured_in_machine_mode() {
    let env = TestEnv::new();
    let output = env.output(&["--json", "switch"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["kind"], "unsupported_flag");
}

#[test]
fn add_api_key_stdin_json_succeeds_without_stderr() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd().args(["init", "--yes"]).assert().success();

    let output = env
        .cmd()
        .args(["add", "claude", "work", "--api-key-stdin", "--json"])
        .write_stdin("sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "add");
    assert_eq!(json["result"]["profile"], "work");
    assert_eq!(json["result"]["credential_backend"], "file");
}

#[test]
fn add_api_key_stdin_empty_is_structured_failure() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd().args(["init", "--yes"]).assert().success();

    let output = env
        .cmd()
        .args(["add", "claude", "work", "--api-key-stdin", "--json"])
        .write_stdin("\n")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["command"], "unknown");
    assert_eq!(json["error"]["kind"], "validation_error");
}

#[test]
fn use_json_returns_active_state_and_backup() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd().args(["init", "--yes"]).assert().success();
    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ])
        .assert()
        .success();

    let json = json_output(&env, &["use", "claude", "work", "--json"]);
    assert_eq!(json["ok"], true);
    assert_eq!(json["result"]["active"]["claude"], "work");
    assert!(json["result"]["backup_ids"].as_array().is_some());
}

#[test]
fn remove_json_returns_remaining_profiles() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd().args(["init", "--yes"]).assert().success();
    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ])
        .assert()
        .success();

    let json = json_output(&env, &["remove", "claude", "work", "--yes", "--json"]);
    assert_eq!(json["ok"], true);
    assert_eq!(json["result"]["removed_profile"], "work");
    assert_eq!(json["result"]["remaining_profiles"], serde_json::json!([]));
}

#[test]
fn rename_json_returns_new_name() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd().args(["init", "--yes"]).assert().success();
    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ])
        .assert()
        .success();

    let json = json_output(&env, &["rename", "claude", "work", "personal", "--json"]);
    assert_eq!(json["ok"], true);
    assert_eq!(json["result"]["new_name"], "personal");
}

#[test]
fn backup_restore_json_reports_non_activation() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd().args(["init", "--yes"]).assert().success();
    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ])
        .assert()
        .success();
    env.cmd().args(["use", "claude", "work"]).assert().success();

    let backups = json_output(&env, &["backup", "list", "--json"]);
    let backup_id = backups[0]["backup_id"].as_str().unwrap().to_owned();

    let json = json_output(&env, &["backup", "restore", &backup_id, "--yes", "--json"]);
    assert_eq!(json["ok"], true);
    assert_eq!(json["result"]["backup_id"], backup_id);
    assert_eq!(json["result"]["activated"], false);
}

#[test]
fn add_oauth_progress_json_streams_ndjson_events() {
    let env = TestEnv::new();
    env.add_script_tool(
        "claude",
        "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"login\" ]; then\n  /bin/mkdir -p \"$HOME/.claude\"\n  printf '%s' '{\"oauthToken\":\"tok\"}' > \"$HOME/.claude/.credentials.json\"\n  exit 0\nfi\necho 'claude 2.3.0'\n",
    );
    env.cmd().args(["init", "--yes"]).assert().success();

    let events = {
        let output = env
            .cmd()
            .args(["add", "claude", "work", "--progress-json"])
            .env("AISW_CLAUDE_AUTH_STORAGE", "file")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>()
    };

    assert_eq!(events[0]["type"], "started");
    assert_eq!(events[0]["tool"], "claude");
    assert_eq!(events[1]["phase"], "starting_upstream_auth");
    assert_eq!(events[2]["type"], "waiting_for_user");
    assert_eq!(events[2]["safe_to_cancel"], true);
    assert_eq!(events[3]["phase"], "applying_changes");
    assert_eq!(events.last().unwrap()["type"], "result");
    assert_eq!(events.last().unwrap()["ok"], true);
    assert_eq!(events.last().unwrap()["result"]["profile"], "work");
}
