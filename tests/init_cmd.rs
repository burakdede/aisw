// Integration tests for `aisw init`.
mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::TestEnv;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn run_init(env: &TestEnv) -> assert_cmd::assert::Assert {
    env.cmd().args(["init", "--yes"]).assert()
}

#[test]
fn init_creates_config_json() {
    let env = TestEnv::new();
    run_init(&env).success();
    env.assert_home_file_exists("config.json");
}

#[test]
fn init_prints_setup_complete() {
    let env = TestEnv::new();
    run_init(&env)
        .success()
        .stdout(contains("Setup complete."))
        .stdout(contains(
            "Next: run 'aisw list' to review profiles, then 'aisw use <tool> <name>' to switch.",
        ));
}

#[test]
fn init_is_idempotent_for_shell_hook() {
    let env = TestEnv::new();
    // Run twice — hook should appear exactly once in the rc file.
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/bin/zsh")
        .assert()
        .success();
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/bin/zsh")
        .assert()
        .success()
        .stdout(contains("already installed"));
}

#[test]
fn init_appends_zsh_hook_to_zshrc() {
    let env = TestEnv::new();
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/bin/zsh")
        .assert()
        .success();

    let zshrc = env.fake_home.join(".zshrc");
    assert!(zshrc.exists(), ".zshrc should be created");
    let contents = fs::read_to_string(&zshrc).unwrap();
    assert!(contents.contains("shell-hook zsh"));
}

#[test]
fn init_appends_fish_hook_to_config_fish() {
    let env = TestEnv::new();
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/usr/bin/fish")
        .assert()
        .success();

    let config_fish = env
        .fake_home
        .join(".config")
        .join("fish")
        .join("config.fish");
    assert!(config_fish.exists(), "config.fish should be created");
    let contents = fs::read_to_string(&config_fish).unwrap();
    assert!(contents.contains("shell-hook fish | source"));
}

#[test]
fn init_unknown_shell_prints_manual_instructions() {
    let env = TestEnv::new();
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/usr/bin/nushell")
        .assert()
        .success()
        .stdout(contains("not recognized"));
}

#[test]
fn init_imports_claude_credentials() {
    let env = TestEnv::new();
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join(".credentials.json"),
        b"{\"token\":\"oauth\"}",
    )
    .unwrap();

    run_init(&env).success().stdout(contains(
        "Imported Claude Code credentials as profile 'default' and marked it active.",
    ));

    let profile_dir = env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default");
    assert!(profile_dir.join(".credentials.json").exists());

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["default"]["auth_method"],
        "o_auth"
    );
    assert_eq!(config["profiles"]["claude"]["default"]["label"], "imported");
    assert_eq!(config["active"]["claude"], "default");
    let live = fs::read(env.fake_home.join(".claude").join(".credentials.json")).unwrap();
    assert_eq!(live, b"{\"token\":\"oauth\"}");
}

#[test]
fn init_imports_codex_credentials() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");
    let codex_dir = env.fake_home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(codex_dir.join("auth.json"), b"{\"token\":\"tok\"}").unwrap();

    run_init(&env).success().stdout(contains(
        "Imported Codex CLI credentials as profile 'default' and marked it active.",
    ));

    let profile_dir = env.aisw_home.join("profiles").join("codex").join("default");
    assert!(profile_dir.join("auth.json").exists());
    assert!(profile_dir.join("config.toml").exists());
    let config_toml = fs::read_to_string(profile_dir.join("config.toml")).unwrap();
    assert!(config_toml.contains("cli_auth_credentials_store = \"file\""));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["codex"]["default"]["auth_method"],
        "o_auth"
    );
    assert_eq!(config["profiles"]["codex"]["default"]["label"], "imported");
    assert_eq!(config["active"]["codex"], "default");

    let live_config = fs::read_to_string(env.fake_home.join(".codex").join("config.toml")).unwrap();
    assert!(live_config.contains("cli_auth_credentials_store = \"file\""));

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains(
        "Codex CLI         default (oauth)           credentials present (validity not checked)",
    ));
}

#[test]
fn init_imports_gemini_env_credentials() {
    let env = TestEnv::new();
    let gemini_dir = env.fake_home.join(".gemini");
    fs::create_dir_all(&gemini_dir).unwrap();
    fs::write(gemini_dir.join(".env"), b"GEMINI_API_KEY=abc\n").unwrap();

    run_init(&env).success().stdout(contains(
        "Imported Gemini CLI credentials as profile 'default' and marked it active.",
    ));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["gemini"]["default"]["auth_method"],
        "api_key"
    );
    assert_eq!(config["profiles"]["gemini"]["default"]["label"], "imported");
    assert_eq!(config["active"]["gemini"], "default");
    let live_env = fs::read_to_string(env.fake_home.join(".gemini").join(".env")).unwrap();
    assert!(live_env.contains("GEMINI_API_KEY=abc"));
}

#[test]
fn init_interactive_import_allows_custom_profile_name_and_label() {
    let env = TestEnv::new();
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join(".credentials.json"),
        b"{\"token\":\"oauth\"}",
    )
    .unwrap();

    env.cmd()
        .arg("init")
        .write_stdin("y\ny\npersonal\nBilling account\n")
        .assert()
        .success()
        .stdout(contains(
            "Imported Claude Code credentials as profile 'personal' and marked it active.",
        ));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["personal"]["label"],
        "Billing account"
    );
    assert_eq!(config["active"]["claude"], "personal");
}

#[test]
fn init_interactive_import_retries_duplicate_profile_name() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
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

    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join(".credentials.json"),
        b"{\"token\":\"oauth\"}",
    )
    .unwrap();

    env.cmd()
        .arg("init")
        .write_stdin("y\ny\nwork\npersonal\nImported fallback\n")
        .assert()
        .success()
        .stderr(contains("already exists"))
        .stdout(contains(
            "Imported Claude Code credentials as profile 'personal' and marked it active.",
        ));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert!(config["profiles"]["claude"]["work"].is_object());
    assert_eq!(
        config["profiles"]["claude"]["personal"]["label"],
        "Imported fallback"
    );
}

#[test]
fn init_does_not_replace_existing_active_profile_when_importing() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "--set-active",
        ])
        .assert()
        .success();

    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join(".credentials.json"),
        b"{\"token\":\"oauth\"}",
    )
    .unwrap();

    run_init(&env)
        .success()
        .stdout(contains(
            "Imported Claude Code credentials as profile 'default'.",
        ))
        .stdout(predicates::str::contains("marked it active.").not());

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "work");
}

#[test]
fn init_imported_credentials_have_600_permissions() {
    let env = TestEnv::new();
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let src = claude_dir.join(".credentials.json");
    fs::write(&src, b"{\"token\":\"oauth\"}").unwrap();
    // Set broad permissions on source to verify we tighten them on copy.
    fs::set_permissions(&src, fs::Permissions::from_mode(0o644)).unwrap();

    run_init(&env).success();

    let dest = env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default")
        .join(".credentials.json");
    env.assert_file_is_600(&dest);
}

#[test]
fn init_skips_import_when_no_credentials_found() {
    let env = TestEnv::new();
    run_init(&env)
        .success()
        .stdout(contains("no existing credentials found."));
}

#[test]
fn init_blocks_import_of_duplicate_oauth_identity() {
    let env = TestEnv::new();

    fs::create_dir_all(&env.aisw_home).unwrap();
    std::fs::write(
        env.aisw_home.join("config.json"),
        serde_json::json!({
            "version": 1,
            "active": { "claude": null, "codex": null, "gemini": null },
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
            "settings": { "backup_on_switch": true, "max_backups": 10 }
        })
        .to_string(),
    )
    .unwrap();

    let profile_dir = env.aisw_home.join("profiles").join("claude").join("work");
    fs::create_dir_all(&profile_dir).unwrap();
    fs::write(
        profile_dir.join(".credentials.json"),
        br#"{"account":{"email":"burak@example.com"}}"#,
    )
    .unwrap();

    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join(".credentials.json"),
        br#"{"account":{"email":"burak@example.com"}}"#,
    )
    .unwrap();

    run_init(&env)
        .failure()
        .stderr(contains("already exists as 'work'"));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default")
        .exists());
}

#[test]
fn init_allows_unresolved_oauth_identity_with_warning() {
    let env = TestEnv::new();
    let codex_dir = env.fake_home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(codex_dir.join("auth.json"), br#"{"token":"opaque-token"}"#).unwrap();

    run_init(&env).success().stderr(contains(
        "could not verify whether codex OAuth profile 'default'",
    ));

    assert!(env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join("default")
        .join("auth.json")
        .exists());
}
