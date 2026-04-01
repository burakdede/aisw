// Integration tests for `aisw init`.
mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::{add_fake_security_tool, TestEnv};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn run_init(env: &TestEnv) -> assert_cmd::assert::Assert {
    env.cmd().args(["init", "--yes"]).assert()
}

fn seed_keyring_item(env: &TestEnv, service: &str, account: &str, secret: &[u8]) {
    let item = env.fake_home.join("keychain").join(service).join(account);
    fs::create_dir_all(&item).unwrap();
    fs::write(item.join("account"), account).unwrap();
    fs::write(item.join("secret"), secret).unwrap();
}

#[test]
fn init_creates_config_json() {
    let env = TestEnv::new();
    run_init(&env).success();
    env.assert_home_file_exists("config.json");
}

#[test]
fn init_non_interactive_without_yes_fails_clearly() {
    let env = TestEnv::new();
    env.cmd()
        .args(["--non-interactive", "init"])
        .assert()
        .failure()
        .stderr(contains("init requires confirmation"))
        .stderr(contains("--yes"));
}

#[test]
fn init_prints_setup_complete() {
    let env = TestEnv::new();
    run_init(&env)
        .success()
        .stdout(contains("Detected tools"))
        .stdout(contains("Credential onboarding"))
        .stdout(contains("Setup complete"))
        .stdout(contains("Next"))
        .stdout(contains("aisw list"))
        .stdout(contains("aisw use <tool> <name>"));
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
#[cfg(target_os = "macos")]
fn init_imports_claude_credentials_from_keychain() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("settings.json"), b"{\"theme\":\"dark\"}").unwrap();
    seed_keyring_item(
        &env,
        "Claude Code-credentials",
        "tester",
        b"{\"token\":\"oauth\"}",
    );

    env.cmd()
        .args(["init", "--yes"])
        .env("AISW_CLAUDE_AUTH_STORAGE", "keychain")
        .env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
        .env("USER", "tester")
        .assert()
        .success()
        .stdout(contains("Local state"))
        .stdout(contains("found"))
        .stdout(contains(".claude"))
        .stdout(contains("found system keyring"))
        .stdout(contains(
            "Imported Claude Code credentials as profile 'default' and marked it active.",
        ));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["default"]["auth_method"],
        "o_auth"
    );
    assert_eq!(
        config["profiles"]["claude"]["default"]["credential_backend"],
        "file"
    );
    assert_eq!(config["active"]["claude"], "default");
    assert!(env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default")
        .join(".credentials.json")
        .exists());
}

#[test]
#[cfg(target_os = "macos")]
fn init_skips_duplicate_claude_keychain_oauth_using_account_metadata() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("settings.json"), b"{\"theme\":\"dark\"}").unwrap();
    fs::write(
        env.fake_home.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"burak@burakdede.com","organizationUuid":"org-123"}}"#,
    )
    .unwrap();
    seed_keyring_item(
        &env,
        "Claude Code-credentials",
        "tester",
        br#"{"claudeAiOauth":{"accessToken":"tok"}}"#,
    );

    env.cmd()
        .args(["init", "--yes"])
        .env("AISW_CLAUDE_AUTH_STORAGE", "keychain")
        .env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
        .env("USER", "tester")
        .assert()
        .success()
        .stdout(contains(
            "Imported Claude Code credentials as profile 'default' and marked it active.",
        ));

    std::fs::write(
        env.aisw_home.join("config.json"),
        serde_json::json!({
            "version": 1,
            "active": { "claude": "burak", "codex": null, "gemini": null },
            "profiles": {
                "claude": {
                    "burak": {
                        "added_at": "2026-03-25T00:00:00Z",
                        "auth_method": "o_auth",
                        "credential_backend": "file",
                        "label": "burak@burakdede.com"
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
    fs::rename(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join("default"),
        env.aisw_home.join("profiles").join("claude").join("burak"),
    )
    .unwrap();

    env.cmd()
        .args(["init", "--yes"])
        .env("AISW_CLAUDE_AUTH_STORAGE", "keychain")
        .env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
        .env("USER", "tester")
        .assert()
        .success()
        .stdout(contains("already managed"))
        .stdout(contains(
            "Current live credentials match stored profile 'burak'.",
        ))
        .stdout(contains(
            "aisw also records 'burak' as the active profile for claude.",
        ));
}

#[test]
fn init_prefers_claude_keychain_over_file_on_macos() {
    if !cfg!(target_os = "macos") {
        return;
    }

    let env = TestEnv::new();
    add_fake_security_tool(&env);
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join(".credentials.json"),
        br#"{"token":"file-oauth"}"#,
    )
    .unwrap();
    seed_keyring_item(
        &env,
        "Claude Code-credentials",
        "tester",
        br#"{"token":"keychain-oauth"}"#,
    );

    env.cmd()
        .args(["init", "--yes"])
        .env("AISW_CLAUDE_AUTH_STORAGE", "keychain")
        .env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
        .env("USER", "tester")
        .assert()
        .success()
        .stdout(contains("found system keyring"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["default"]["credential_backend"],
        "file"
    );
    assert!(env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default")
        .join(".credentials.json")
        .exists());
}

#[test]
fn init_reports_claude_local_state_without_importable_auth() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("settings.json"), b"{\"theme\":\"dark\"}").unwrap();

    env.cmd()
        .args(["init", "--yes"])
        .env("AISW_CLAUDE_AUTH_STORAGE", "keychain")
        .env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
        .env("USER", "tester")
        .assert()
        .success()
        .stdout(contains("Claude Code"))
        .stdout(contains("Local state"))
        .stdout(contains(".claude"))
        .stdout(contains("not found in file or Keychain"))
        .stdout(contains("could not find importable auth"));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default")
        .exists());
}

#[test]
fn init_reports_codex_local_state_without_importable_auth() {
    let env = TestEnv::new();
    let codex_dir = env.fake_home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(
        codex_dir.join("config.toml"),
        b"cli_auth_credentials_store = \"keyring\"\n",
    )
    .unwrap();

    run_init(&env)
        .success()
        .stdout(contains("Codex CLI"))
        .stdout(contains("Local state"))
        .stdout(contains(".codex"))
        .stdout(contains("not found in auth.json"))
        .stdout(contains("Auth storage"))
        .stdout(contains("keyring"))
        .stdout(contains("keyring-backed auth"))
        .stdout(contains("could not locate a readable credential there"));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join("default")
        .exists());
}

#[test]
fn init_reports_codex_auto_backend_without_importable_auth() {
    let env = TestEnv::new();
    let codex_dir = env.fake_home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(codex_dir.join("config.toml"), b"model = \"gpt-5.4\"\n").unwrap();

    run_init(&env)
        .success()
        .stdout(contains("Codex CLI"))
        .stdout(contains("Local state"))
        .stdout(contains(".codex"))
        .stdout(contains("not found in auth.json"))
        .stdout(contains("Auth storage"))
        .stdout(contains("auto"))
        .stdout(contains("may be using the system keyring"));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join("default")
        .exists());
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
        "api_key"
    );
    assert_eq!(config["profiles"]["codex"]["default"]["label"], "imported");
    assert_eq!(config["active"]["codex"], "default");
    assert_eq!(
        config["profiles"]["codex"]["default"]["credential_backend"],
        "file"
    );

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("Codex CLI"))
        .stdout(contains("Active"))
        .stdout(contains("default"))
        .stdout(contains("Auth"))
        .stdout(contains("api_key"))
        .stdout(contains(
            "live tool config does not match the active profile",
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
fn init_imports_gemini_oauth_credentials_from_oauth_creds_file() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");
    let gemini_dir = env.fake_home.join(".gemini");
    fs::create_dir_all(&gemini_dir).unwrap();
    fs::write(
        gemini_dir.join("oauth_creds.json"),
        br#"{"email":"burak@example.com","access_token":"tok"}"#,
    )
    .unwrap();

    run_init(&env)
        .success()
        .stdout(contains("Gemini CLI"))
        .stdout(contains("oauth"))
        .stdout(contains(
            "Imported Gemini CLI credentials as profile 'default' and marked it active.",
        ));

    let profile_dir = env
        .aisw_home
        .join("profiles")
        .join("gemini")
        .join("default");
    assert!(profile_dir.join("oauth_creds.json").exists());

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["gemini"]["default"]["auth_method"],
        "o_auth"
    );
    assert_eq!(config["active"]["gemini"], "default");
}

#[test]
fn init_imports_all_gemini_oauth_cache_files() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");
    let gemini_dir = env.fake_home.join(".gemini");
    fs::create_dir_all(&gemini_dir).unwrap();
    fs::write(
        gemini_dir.join("settings.json"),
        br#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#,
    )
    .unwrap();
    fs::write(
        gemini_dir.join("oauth_creds.json"),
        br#"{"email":"burak@example.com","access_token":"tok"}"#,
    )
    .unwrap();

    run_init(&env).success();

    let profile_dir = env
        .aisw_home
        .join("profiles")
        .join("gemini")
        .join("default");
    assert!(profile_dir.join("settings.json").exists());
    assert!(profile_dir.join("oauth_creds.json").exists());
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
        .stdout(contains("Claude Code"))
        .stdout(contains("Credentials"))
        .stdout(contains("not found"));
}

#[test]
fn init_reports_detected_tools_and_missing_tools_explicitly() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    run_init(&env)
        .success()
        .stdout(contains("Detected tools"))
        .stdout(contains("Codex CLI"))
        .stdout(contains("Status"))
        .stdout(contains("detected"))
        .stdout(contains("Claude Code"))
        .stdout(contains("not detected"))
        .stdout(contains("Gemini CLI"));
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
        .success()
        .stdout(contains("already managed"))
        .stdout(contains(
            "Current live credentials match stored profile 'work'.",
        ))
        .stdout(contains(
            "aisw does not currently record an active profile for claude.",
        ));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default")
        .exists());
}

#[test]
fn init_skips_duplicate_codex_oauth_identity_without_failing() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    fs::create_dir_all(&env.aisw_home).unwrap();
    std::fs::write(
        env.aisw_home.join("config.json"),
        serde_json::json!({
            "version": 1,
            "active": { "claude": null, "codex": "burak", "gemini": null },
            "profiles": {
                "claude": {},
                "codex": {
                    "burak": {
                        "added_at": "2026-03-25T00:00:00Z",
                        "auth_method": "o_auth",
                        "label": null
                    }
                },
                "gemini": {}
            },
            "settings": { "backup_on_switch": true, "max_backups": 10 }
        })
        .to_string(),
    )
    .unwrap();

    let profile_dir = env.aisw_home.join("profiles").join("codex").join("burak");
    fs::create_dir_all(&profile_dir).unwrap();
    fs::write(
        profile_dir.join("auth.json"),
        br#"{"account":{"email":"burak@burakdede.com"}}"#,
    )
    .unwrap();
    fs::write(
        profile_dir.join("config.toml"),
        "cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();

    let codex_dir = env.fake_home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(
        codex_dir.join("auth.json"),
        br#"{"account":{"email":"burak@burakdede.com"}}"#,
    )
    .unwrap();

    run_init(&env)
        .success()
        .stdout(contains("Codex CLI"))
        .stdout(contains("already managed"))
        .stdout(contains(
            "Current live credentials match stored profile 'burak'.",
        ))
        .stdout(contains(
            "aisw also records 'burak' as the active profile for codex.",
        ));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join("default")
        .exists());
}

#[test]
fn init_skips_duplicate_claude_api_key_without_failing() {
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
        br#"{"apiKey":"sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
    )
    .unwrap();

    run_init(&env)
        .success()
        .stdout(contains("Claude Code"))
        .stdout(contains("already managed"))
        .stdout(contains(
            "Current live credentials match stored profile 'work'.",
        ))
        .stdout(contains(
            "aisw does not currently record an active profile for claude.",
        ));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default")
        .exists());
}

#[test]
fn init_skips_duplicate_gemini_api_key_without_failing() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args([
            "add",
            "gemini",
            "work",
            "--api-key",
            "AIzatest1234567890ABCDEF",
        ])
        .assert()
        .success();

    let gemini_dir = env.fake_home.join(".gemini");
    fs::create_dir_all(&gemini_dir).unwrap();
    fs::write(
        gemini_dir.join(".env"),
        b"GEMINI_API_KEY=AIzatest1234567890ABCDEF\n",
    )
    .unwrap();

    run_init(&env)
        .success()
        .stdout(contains("Gemini CLI"))
        .stdout(contains("already managed"))
        .stdout(contains(
            "Current live credentials match stored profile 'work'.",
        ))
        .stdout(contains(
            "aisw does not currently record an active profile for gemini.",
        ));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("gemini")
        .join("default")
        .exists());
}

#[test]
fn init_skips_duplicate_gemini_oauth_identity_without_failing() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    fs::create_dir_all(&env.aisw_home).unwrap();
    std::fs::write(
        env.aisw_home.join("config.json"),
        serde_json::json!({
            "version": 1,
            "active": { "claude": null, "codex": null, "gemini": "work" },
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
            "settings": { "backup_on_switch": true, "max_backups": 10 }
        })
        .to_string(),
    )
    .unwrap();

    let profile_dir = env.aisw_home.join("profiles").join("gemini").join("work");
    fs::create_dir_all(&profile_dir).unwrap();
    fs::write(
        profile_dir.join("oauth_creds.json"),
        br#"{"email":"burak@example.com","access_token":"tok"}"#,
    )
    .unwrap();

    let gemini_dir = env.fake_home.join(".gemini");
    fs::create_dir_all(&gemini_dir).unwrap();
    fs::write(
        gemini_dir.join("oauth_creds.json"),
        br#"{"email":"burak@example.com","access_token":"tok"}"#,
    )
    .unwrap();

    run_init(&env)
        .success()
        .stdout(contains("Gemini CLI"))
        .stdout(contains("already managed"))
        .stdout(contains(
            "Current live credentials match stored profile 'work'.",
        ))
        .stdout(contains(
            "aisw also records 'work' as the active profile for gemini.",
        ));

    assert!(!env
        .aisw_home
        .join("profiles")
        .join("gemini")
        .join("default")
        .exists());
}

#[test]
fn init_allows_unresolved_oauth_identity_without_warning() {
    let env = TestEnv::new();
    let codex_dir = env.fake_home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(codex_dir.join("auth.json"), br#"{"token":"opaque-token"}"#).unwrap();

    run_init(&env).success();

    assert!(env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join("default")
        .join("auth.json")
        .exists());
}
