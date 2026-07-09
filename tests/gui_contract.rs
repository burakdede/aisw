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
    assert_eq!(json["features"]["verify"], true);
    assert_eq!(json["features"]["repair"], true);
    assert_eq!(json["features"]["project_bindings_alias"], true);
    assert_eq!(json["tools"]["gemini"]["state_modes"][0], "isolated");
    assert_eq!(json["tools"]["codex"]["fail_closed_keyring_identity"], true);
}

#[test]
fn workspace_bind_json_returns_binding_and_snapshot() {
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
    env.cmd()
        .args(["context", "create", "client-acme", "--claude", "work"])
        .assert()
        .success();

    let json = json_output(
        &env,
        &[
            "workspace",
            "bind",
            "--git-remote",
            "git@github.com:acme/*",
            "--context",
            "client-acme",
            "--json",
        ],
    );
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "workspace_bind");
    assert_eq!(json["result"]["binding"]["scope"], "git_remote");
    assert_eq!(json["result"]["binding"]["pattern"], "github.com/acme/*");
    assert_eq!(
        json["result"]["project_bindings"]["user_bindings"]["guard_mode"],
        "warn"
    );
}

#[test]
fn workspace_guard_json_returns_updated_mode_and_snapshot() {
    let env = TestEnv::new();

    let json = json_output(&env, &["workspace", "guard", "--mode", "strict", "--json"]);
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "workspace_guard");
    assert_eq!(json["result"]["guard_mode"], "strict");
    assert_eq!(
        json["result"]["project_bindings"]["user_bindings"]["guard_mode"],
        "strict"
    );
}

#[test]
fn verify_json_reports_failures_and_remediation() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.add_fake_tool("codex", "codex 1.0.0");
    env.add_fake_tool("gemini", "gemini 0.9.0");
    env.cmd().args(["init", "--yes"]).assert().success();
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

    std::fs::write(
        env.fake_home.join(".codex").join("auth.json"),
        br#"{"token":"different"}"#,
    )
    .unwrap();

    let output = env.output(&["verify", "--json"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["summary"]["status"], "fail");
    assert!(json["doctor"].is_array());
    let codex = json["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["tool"] == "codex")
        .unwrap();
    assert_eq!(codex["status"], "fail");
    assert!(codex["issues"][0]
        .as_str()
        .unwrap()
        .contains("live tool credentials do not match"));
    assert!(codex["remediation"][0]
        .as_str()
        .unwrap()
        .contains("aisw use codex work"));
}

#[test]
fn repair_json_dry_run_reports_planned_safe_fixes() {
    let env = TestEnv::new();
    let missing_home = env.dir.path().join("missing-aisw-home");

    let output = env
        .cmd()
        .env("AISW_HOME", &missing_home)
        .args(["repair", "--json", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "repair");
    assert_eq!(json["result"]["mode"], "dry_run");
    assert_eq!(json["result"]["summary"]["status"], "warn");
    assert_eq!(json["result"]["summary"]["issues_remaining"], 2);
    assert_eq!(json["result"]["actions"][0]["kind"], "create_dir");
    assert!(!missing_home.exists());
}

#[test]
fn repair_json_apply_creates_home_and_config() {
    let env = TestEnv::new();
    let missing_home = env.dir.path().join("missing-aisw-home");

    let output = env
        .cmd()
        .env("AISW_HOME", &missing_home)
        .args(["repair", "--json", "--apply", "--fix", "home"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["result"]["mode"], "apply");
    assert_eq!(json["result"]["summary"]["status"], "pass");
    assert!(missing_home.join("config.json").exists());
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
fn add_duplicate_profile_is_structured_in_machine_mode() {
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

    let output = env.output(&[
        "add",
        "claude",
        "work",
        "--api-key",
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "--json",
    ]);

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["kind"], "profile_already_exists");
    assert_eq!(json["error"]["remediation"]["command"], "aisw list claude");
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
fn use_missing_profile_is_structured_in_machine_mode() {
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

    let output = env.output(&["use", "claude", "wrok", "--json"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["kind"], "profile_not_found");
    assert_eq!(json["error"]["remediation"]["command"], "aisw list claude");
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
fn remove_missing_profile_is_structured_in_machine_mode() {
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

    let output = env.output(&["remove", "claude", "wrok", "--yes", "--json"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["kind"], "profile_not_found");
    assert_eq!(json["error"]["remediation"]["command"], "aisw list claude");
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
fn rename_missing_profile_is_structured_in_machine_mode() {
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

    let output = env.output(&["rename", "claude", "wrok", "personal", "--json"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["kind"], "profile_not_found");
    assert_eq!(json["error"]["remediation"]["command"], "aisw list claude");
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
fn context_create_json_returns_saved_mapping() {
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

    let json = json_output(
        &env,
        &[
            "context",
            "create",
            "client-acme",
            "--claude",
            "work",
            "--json",
        ],
    );
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "context_create");
    assert_eq!(json["result"]["context"]["name"], "client-acme");
    assert_eq!(json["result"]["context"]["profiles"]["claude"], "work");
}

#[test]
fn context_use_json_returns_active_state_and_context_name() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd().args(["init", "--yes"]).assert().success();
    env.cmd()
        .args([
            "add",
            "claude",
            "work-claude",
            "--api-key",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "codex",
            "work-codex",
            "--api-key",
            "sk-codex-test-key-12345",
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "work-claude",
            "--codex",
            "work-codex",
        ])
        .assert()
        .success();

    let json = json_output(&env, &["context", "use", "work", "--json"]);
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "context_use");
    assert_eq!(json["result"]["context"], "work");
    assert_eq!(json["result"]["active"]["claude"], "work-claude");
    assert_eq!(json["result"]["active"]["codex"], "work-codex");
    assert!(json["result"]["backup_ids"].as_array().is_some());
}

#[test]
fn context_remove_json_returns_remaining_contexts() {
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
    env.cmd()
        .args(["context", "create", "client-acme", "--claude", "work"])
        .assert()
        .success();

    let json = json_output(
        &env,
        &["context", "remove", "client-acme", "--yes", "--json"],
    );
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "context_remove");
    assert_eq!(json["result"]["removed_context"], "client-acme");
    assert_eq!(json["result"]["remaining_contexts"], serde_json::json!([]));
}

#[test]
fn project_bindings_list_json_reports_user_and_repo_rules() {
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
    env.cmd()
        .args(["context", "create", "client-acme", "--claude", "work"])
        .assert()
        .success();

    let repo = env.fake_home.join("clients").join("acme");
    std::fs::create_dir_all(repo.join(".git").join("info")).unwrap();
    std::fs::write(
        repo.join(".git").join("config"),
        "[remote \"origin\"]\n\turl = git@github.com:acme/api.git\n",
    )
    .unwrap();

    env.cmd()
        .args(["workspace", "bind", "--default", "--context", "client-acme"])
        .assert()
        .success();
    env.cmd()
        .args([
            "workspace",
            "bind",
            "--git-remote",
            "github.com/acme/*",
            "--context",
            "client-acme",
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "workspace",
            "bind",
            repo.to_str().unwrap(),
            "--context",
            "client-acme",
        ])
        .assert()
        .success();

    let output = env
        .cmd()
        .current_dir(&repo)
        .args(["project-bindings", "list", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "project_bindings_list");
    assert_eq!(
        json["result"]["repo_local_binding"]["context"],
        "client-acme"
    );
    assert_eq!(
        json["result"]["user_bindings"]["default_context"],
        "client-acme"
    );
    assert_eq!(
        json["result"]["user_bindings"]["git_remote_rules"][0]["pattern"],
        "github.com/acme/*"
    );
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
