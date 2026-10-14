mod common;

use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const ANTIGRAVITY_SECRET: &str = r#"{"email":"work@example.com","token":"work-live"}"#;

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

fn antigravity_live_secret_path(env: &TestEnv) -> std::path::PathBuf {
    let account = if cfg!(windows) {
        let encoded: String = b"antigravity"
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!("h_{encoded}")
    } else {
        "antigravity".to_owned()
    };
    env.fake_home
        .join("keychain/gemini")
        .join(account)
        .join("secret")
}

fn write_antigravity_live_state(env: &TestEnv, secret: &str, theme: &str) {
    let app_dir = env.fake_home.join(".gemini").join("antigravity-cli");
    let shared_dir = env.fake_home.join(".gemini").join("config");
    std::fs::create_dir_all(app_dir.join("cache")).unwrap();
    std::fs::create_dir_all(shared_dir.join("projects")).unwrap();
    std::fs::write(
        app_dir.join("settings.json"),
        format!(r#"{{"theme":"{theme}"}}"#),
    )
    .unwrap();
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
    let secret_path = antigravity_live_secret_path(env);
    std::fs::create_dir_all(secret_path.parent().unwrap()).unwrap();
    std::fs::write(secret_path.parent().unwrap().join("account"), "antigravity").unwrap();
    std::fs::write(secret_path, secret).unwrap();
}

fn add_antigravity_profile(env: &TestEnv, name: &str) {
    env.add_fake_tool("agy", "agy 1.0.0");
    write_antigravity_live_state(env, ANTIGRAVITY_SECRET, "terminal");
    env.cmd()
        .args(["add", "antigravity", name, "--from-live"])
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
fn context_use_with_antigravity_updates_live_state_and_active_profile() {
    let env = TestEnv::new();
    setup_profiles(&env);
    add_antigravity_profile(&env, "acme-antigravity");

    env.cmd()
        .args([
            "context",
            "create",
            "work",
            "--claude",
            "acme-claude",
            "--antigravity",
            "acme-antigravity",
        ])
        .assert()
        .success();

    write_antigravity_live_state(
        &env,
        r#"{"email":"other@example.com","token":"other"}"#,
        "light",
    );

    env.cmd()
        .args(["context", "use", "work"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(
            env.fake_home
                .join(".gemini")
                .join("antigravity-cli")
                .join("settings.json")
        )
        .unwrap(),
        r#"{"theme":"light"}"#
    );
    assert_eq!(
        std::fs::read_to_string(antigravity_live_secret_path(&env)).unwrap(),
        ANTIGRAVITY_SECRET
    );

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "acme-claude");
    assert_eq!(config["active"]["antigravity"], "acme-antigravity");
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

#[test]
fn gemini_and_antigravity_context_restores_shared_root_after_late_failure() {
    let env = TestEnv::new();
    setup_profiles(&env);
    let gemini = env.fake_home.join(".gemini");
    std::fs::create_dir_all(&gemini).unwrap();

    std::fs::write(
        gemini.join("oauth_creds.json"),
        br#"{"email":"work-gemini@example.com","token":"work"}"#,
    )
    .unwrap();
    std::fs::write(
        gemini.join("settings.json"),
        br#"{"security":{"auth":{"selectedType":"oauth-personal"}},"theme":"work"}"#,
    )
    .unwrap();

    env.cmd()
        .args(["add", "gemini", "work-oauth", "--from-live"])
        .assert()
        .success();
    for root in [
        gemini.clone(),
        env.aisw_home.join("profiles/gemini/work-oauth"),
    ] {
        std::fs::create_dir_all(root.join("cache/nested")).unwrap();
        std::fs::write(root.join("cache/nested/state.json"), b"work-cache").unwrap();
    }
    add_antigravity_profile(&env, "work-agy");
    env.cmd()
        .args([
            "add",
            "antigravity",
            "personal-agy",
            "--api-key",
            "AIzapersonal1234567890ABCDEF",
        ])
        .assert()
        .success();
    env.cmd()
        .args(["use", "gemini", "acme-gemini"])
        .assert()
        .success();
    env.cmd()
        .args(["use", "antigravity", "personal-agy"])
        .assert()
        .success();
    std::fs::write(gemini.join("cache/nested/state.json"), b"personal-cache").unwrap();
    std::fs::write(gemini.join("antigravity-cli/settings.json"),
        br#"{"modelProvider":"gemini","theme":"night","mcpServers":{"local":{"command":"local-tool"}}}"#).unwrap();
    std::fs::write(
        gemini.join("antigravity-cli/cache/projects.json"),
        br#"{"current":"personal-project"}"#,
    )
    .unwrap();
    std::fs::write(
        gemini.join("config/hooks.json"),
        br#"{"hooks":["personal"]}"#,
    )
    .unwrap();
    std::fs::write(
        gemini.join("config/projects/repo.json"),
        br#"{"mode":"act"}"#,
    )
    .unwrap();

    std::fs::create_dir_all(gemini.join("runtime/session")).unwrap();
    std::fs::write(
        gemini.join("runtime/session/current.json"),
        b"personal-runtime",
    )
    .unwrap();
    let live_secret = antigravity_live_secret_path(&env);
    std::fs::write(
        &live_secret,
        br#"{"email":"personal-agy@example.com","token":"personal"}"#,
    )
    .unwrap();
    env.cmd()
        .args([
            "context",
            "create",
            "work-both",
            "--gemini",
            "work-oauth",
            "--antigravity",
            "work-agy",
        ])
        .assert()
        .success();

    let protected_paths = [
        gemini.join(".env"),
        gemini.join("oauth_creds.json"),
        gemini.join("settings.json"),
        gemini.join("cache/nested/state.json"),
        gemini.join("runtime/session/current.json"),
        gemini.join("antigravity-cli/settings.json"),
        gemini.join("antigravity-cli/cache/projects.json"),
        gemini.join("config/hooks.json"),
        gemini.join("config/projects/repo.json"),
        live_secret.clone(),
    ];
    let before = protected_paths.map(|path| {
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes)
    });
    let config_before: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();

    env.cmd()
        .env("AISW_FAULT_INJECTION", "live_apply.commit_write:4")
        .args(["context", "use", "work-both"])
        .assert()
        .failure()
        .stderr(contains("injected live-apply failure"));
    for (path, bytes) in &before {
        assert_eq!(std::fs::read(path).unwrap(), *bytes, "{}", path.display());
    }
    let config_after: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config_after["active"], config_before["active"]);

    env.cmd()
        .args(["context", "use", "work-both", "--json"])
        .assert()
        .success();
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(gemini.join("settings.json")).unwrap()).unwrap();
    assert_eq!(
        settings["security"]["auth"]["selectedType"],
        "oauth-personal"
    );
    assert_eq!(
        std::fs::read(gemini.join("cache/nested/state.json")).unwrap(),
        b"work-cache"
    );
    assert!(!gemini.join("runtime/session/current.json").exists());
    let agy_settings: serde_json::Value = serde_json::from_slice(
        &std::fs::read(gemini.join("antigravity-cli/settings.json")).unwrap(),
    )
    .unwrap();
    assert!(agy_settings.get("modelProvider").is_none());
    assert_eq!(agy_settings["theme"], "night");
    assert_eq!(agy_settings["mcpServers"]["local"]["command"], "local-tool");
    for (path, bytes) in &before[6..9] {
        assert_eq!(std::fs::read(path).unwrap(), *bytes, "{}", path.display());
    }
    assert_eq!(
        std::fs::read_to_string(live_secret).unwrap(),
        ANTIGRAVITY_SECRET
    );
    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["gemini"], "work-oauth");
    assert_eq!(config["active"]["antigravity"], "work-agy");
}
