// Integration tests for `aisw add` across all tools.
mod common;

use std::fs;
use std::path::PathBuf;

use common::assert_output_redacts_secret;
use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";
const VALID_CODEX_KEY_ALT: &str = "sk-codex-test-key-67890";
const VALID_GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";
const VALID_GEMINI_KEY_ALT: &str = "AIzaalt0987654321FEDCBA";
const ANTIGRAVITY_SECRET: &str = r#"{"email":"work@example.com","token":"live"}"#;
const ANTIGRAVITY_SECRET_ALT: &str = r#"{"email":"personal@example.com","token":"other"}"#;

fn antigravity_live_keyring_secret_path(env: &TestEnv) -> PathBuf {
    env.fake_home
        .join("keychain")
        .join("gemini")
        .join("antigravity")
        .join("secret")
}

fn write_antigravity_live_state(env: &TestEnv, secret: &str) {
    let app_dir = env.fake_home.join(".gemini").join("antigravity-cli");
    let shared_dir = env.fake_home.join(".gemini").join("config");
    fs::create_dir_all(app_dir.join("cache")).unwrap();
    fs::create_dir_all(shared_dir.join("projects")).unwrap();
    fs::write(app_dir.join("settings.json"), br#"{"theme":"terminal"}"#).unwrap();
    fs::write(
        app_dir.join("cache").join("projects.json"),
        br#"{"current":"repo"}"#,
    )
    .unwrap();
    fs::write(shared_dir.join("hooks.json"), br#"{"hooks":[]}"#).unwrap();
    fs::write(
        shared_dir.join("projects").join("repo.json"),
        br#"{"mode":"plan"}"#,
    )
    .unwrap();

    let secret_path = antigravity_live_keyring_secret_path(env);
    fs::create_dir_all(secret_path.parent().unwrap()).unwrap();
    fs::write(secret_path.parent().unwrap().join("account"), "antigravity").unwrap();
    fs::write(secret_path, secret).unwrap();
}

fn write_config_only_profile(env: &TestEnv, tool: &str, profile: &str, backend: &str) {
    let mut config = serde_json::json!({
        "version": 2,
        "active": {"claude": null, "codex": null, "gemini": null},
        "profiles": {"claude": {}, "codex": {}, "gemini": {}},
        "contexts": {},
        "settings": {
            "backup_on_switch": true,
            "max_backups": 10,
            "tool_settings": {
                "claude": {"state_mode": "isolated"},
                "codex": {"state_mode": "isolated"}
            }
        }
    });
    config["profiles"][tool][profile] = serde_json::json!({
        "added_at": "2026-01-01T00:00:00Z",
        "auth_method": "api_key",
        "credential_backend": backend,
        "label": null
    });
    fs::write(
        env.home_file("config.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

fn keyring_account_component(account: &str) -> String {
    if !cfg!(windows) {
        return account.to_owned();
    }

    let mut encoded = String::with_capacity(2 + account.len() * 2);
    encoded.push_str("h_");
    for byte in account.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn aisw_keyring_secret_path(env: &TestEnv, tool: &str, profile: &str) -> PathBuf {
    let account = format!("profile:{tool}:{profile}");
    env.fake_home
        .join("keychain")
        .join("aisw")
        .join(keyring_account_component(&account))
        .join("secret")
}

fn seed_aisw_keyring_secret(env: &TestEnv, tool: &str, profile: &str, secret: &str) -> PathBuf {
    let path = aisw_keyring_secret_path(env, tool, profile);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path.parent().unwrap().join("account"),
        format!("profile:{tool}:{profile}"),
    )
    .unwrap();
    fs::write(&path, secret).unwrap();
    path
}

// ---- Claude ----

#[test]
fn add_claude_api_key_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("Tool"))
        .stdout(contains("Claude Code"))
        .stdout(contains("work"))
        .stdout(contains("Next"))
        .stdout(contains("aisw use claude work"));
}

#[test]
fn add_claude_tool_not_installed_fails() {
    // No claude binary added to PATH.
    TestEnv::new()
        .cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .failure()
        .stderr(contains("not installed"));
}

#[test]
fn add_claude_api_key_with_set_active() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            VALID_CLAUDE_KEY,
            "--set-active",
        ])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "work");
}

#[test]
fn add_claude_api_key_with_label() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            VALID_CLAUDE_KEY,
            "--label",
            "My work account",
        ])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["work"]["label"],
        "My work account"
    );
}

#[test]
fn add_invalid_profile_name_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    // Space in profile name is invalid.
    env.cmd()
        .args(["add", "claude", "my profile", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .failure();
}

#[test]
fn add_duplicate_profile_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

#[test]
fn add_duplicate_claude_api_key_under_different_name_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();

    let output = env.output(&["add", "claude", "backup", "--api-key", VALID_CLAUDE_KEY]);
    assert!(!output.status.success(), "duplicate add should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("API key already exists as profile 'work'"));
    assert_output_redacts_secret(&output, VALID_CLAUDE_KEY);
}

#[test]
fn add_distinct_claude_api_keys_under_different_names_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success();

    env.cmd()
        .args([
            "add",
            "claude",
            "personal",
            "--api-key",
            VALID_CLAUDE_KEY_ALT,
        ])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("personal"));
}

#[test]
fn add_claude_oauth_succeeds_with_mocked_binary() {
    let env = TestEnv::new();
    env.add_script_tool(
        "claude",
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then\n\
           echo 'claude 2.3.0'\n\
           exit 0\n\
         fi\n\
         [ \"$1\" = \"auth\" ] || exit 9\n\
         [ \"$2\" = \"login\" ] || exit 8\n\
         target_dir=\"${CLAUDE_CONFIG_DIR:-$HOME/.claude}\"\n\
         /bin/mkdir -p \"$target_dir\"\n\
         printf '%s' '{\"oauthToken\":\"tok\"}' > \"$target_dir/.credentials.json\"\n",
    );

    env.cmd()
        .env("AISW_CLAUDE_AUTH_STORAGE", "file")
        .args(["add", "claude", "work"])
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["work"]["auth_method"],
        "o_auth"
    );
    env.assert_home_file_exists("profiles/claude/work/.credentials.json");
}

#[test]
#[cfg(target_os = "macos")]
fn add_claude_oauth_succeeds_with_keychain_backed_credentials() {
    let env = TestEnv::new();
    env.add_script_tool(
        "claude",
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then\n\
           echo 'claude 2.3.0'\n\
           exit 0\n\
         fi\n\
         [ \"$1\" = \"auth\" ] || exit 9\n\
         [ \"$2\" = \"login\" ] || exit 8\n\
         item=\"$AISW_KEYRING_TEST_DIR/Claude Code-credentials/${USER:-tester}\"\n\
         /bin/mkdir -p \"$item\"\n\
         printf '%s' \"${USER:-tester}\" > \"$item/account\"\n\
         printf '%s' '{\"account\":{\"email\":\"work@example.com\"}}' > \"$item/secret\"\n",
    );

    env.cmd()
        .env("AISW_CLAUDE_AUTH_STORAGE", "keychain")
        .env("AISW_CLAUDE_KEYCHAIN_SCHEME", "shared")
        .env("USER", "tester")
        .args(["add", "claude", "work"])
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["work"]["credential_backend"],
        "file"
    );
    assert!(
        env.home_file("profiles/claude/work/.credentials.json")
            .exists(),
        "Claude OAuth profile should persist a managed credentials file on macOS",
    );
}

#[test]
fn add_claude_recovers_from_unmanaged_orphaned_profile_dir() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    std::fs::create_dir_all(env.aisw_home.join("profiles").join("claude").join("work")).unwrap();

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["work"]["auth_method"],
        "api_key"
    );
}

#[test]
fn add_claude_rejects_config_only_duplicate_before_writing_system_keyring() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    write_config_only_profile(&env, "claude", "work", "system_keyring");
    let secret_path = seed_aisw_keyring_secret(
        &env,
        "claude",
        "work",
        r#"{"apiKey":"sk-ant-api03-OLDOLDOLDOLDOLDOLDOLDOLD"}"#,
    );

    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            VALID_CLAUDE_KEY,
            "--credential-backend",
            "system-keyring",
        ])
        .assert()
        .failure()
        .stderr(contains("already exists"));

    assert_eq!(
        fs::read_to_string(&secret_path).unwrap(),
        r#"{"apiKey":"sk-ant-api03-OLDOLDOLDOLDOLDOLDOLDOLD"}"#
    );
    assert!(
        !env.home_file("profiles/claude/work").exists(),
        "config-only duplicate must not create a profile directory"
    );
}

#[test]
fn add_claude_rejects_config_only_duplicate_before_writing_file_profile() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");
    write_config_only_profile(&env, "claude", "work", "file");

    env.cmd()
        .args(["add", "claude", "work", "--api-key", VALID_CLAUDE_KEY])
        .assert()
        .failure()
        .stderr(contains("already exists"));

    assert!(
        !env.home_file("profiles/claude/work").exists(),
        "config-only duplicate must not create a profile directory"
    );
}

// ---- Codex ----

#[test]
fn add_codex_api_key_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("Tool"))
        .stdout(contains("Codex CLI"))
        .stdout(contains("work"))
        .stdout(contains("Next"))
        .stdout(contains("aisw use codex work"));
}

#[test]
fn add_codex_tool_not_installed_fails() {
    TestEnv::new()
        .cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .failure()
        .stderr(contains("not installed"));
}

#[test]
fn add_duplicate_codex_api_key_under_different_name_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();

    let output = env.output(&["add", "codex", "backup", "--api-key", VALID_CODEX_KEY]);
    assert!(!output.status.success(), "duplicate add should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("API key already exists as profile 'work'"));
    assert_output_redacts_secret(&output, VALID_CODEX_KEY);
}

#[test]
fn add_distinct_codex_api_keys_under_different_names_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args(["add", "codex", "work", "--api-key", VALID_CODEX_KEY])
        .assert()
        .success();

    env.cmd()
        .args(["add", "codex", "personal", "--api-key", VALID_CODEX_KEY_ALT])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("personal"));
}

#[test]
fn add_codex_oauth_succeeds_with_mocked_binary() {
    let env = TestEnv::new();
    env.add_script_tool(
        "codex",
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then\n\
           echo 'codex 1.0.0'\n\
           exit 0\n\
         fi\n\
         if [ \"$1\" = \"login\" ]; then\n\
           /bin/mkdir -p \"$CODEX_HOME\"\n\
           printf '%s' '{\"token\":\"tok\"}' > \"$CODEX_HOME/auth.json\"\n\
           exit 0\n\
         fi\n\
         exit 1\n",
    );

    env.cmd()
        .args(["add", "codex", "work"])
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["profiles"]["codex"]["work"]["auth_method"], "o_auth");
    assert_eq!(
        config["profiles"]["codex"]["work"]["credential_backend"],
        "file"
    );
    env.assert_home_file_exists("profiles/codex/work/auth.json");
    env.assert_home_file_exists("profiles/codex/work/config.toml");
}

#[test]
fn add_codex_oauth_always_uses_file_backend() {
    // Codex stores credentials in a path-hash-keyed keyring entry that aisw
    // cannot reconstruct; aisw therefore always uses file-backed storage for
    // Codex OAuth profiles regardless of any override hints.
    let env = TestEnv::new();
    env.add_script_tool(
        "codex",
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then\n\
           echo 'codex 1.0.0'\n\
           exit 0\n\
         fi\n\
         if [ \"$1\" = \"login\" ]; then\n\
           /bin/mkdir -p \"$CODEX_HOME\"\n\
           printf '%s' '{\"token\":\"tok\",\"email\":\"work@example.com\"}' > \"$CODEX_HOME/auth.json\"\n\
           exit 0\n\
         fi\n\
         exit 1\n",
    );

    env.cmd()
        .args(["add", "codex", "work"])
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["profiles"]["codex"]["work"]["auth_method"], "o_auth");
    assert_eq!(
        config["profiles"]["codex"]["work"]["credential_backend"],
        "file"
    );
    env.assert_home_file_exists("profiles/codex/work/auth.json");
    env.assert_home_file_exists("profiles/codex/work/config.toml");
    assert!(!env
        .home_file("profiles/codex/work/.oauth-capture/auth.json")
        .exists());
}

#[test]
fn add_claude_api_key_supports_explicit_system_keyring_backend() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            VALID_CLAUDE_KEY,
            "--credential-backend",
            "system-keyring",
        ])
        .assert()
        .success()
        .stdout(contains("Backend"))
        .stdout(contains("system_keyring"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["work"]["credential_backend"],
        "system_keyring"
    );
    assert!(
        !env.home_file("profiles/claude/work/.credentials.json")
            .exists(),
        "keyring-backed Claude API key profile should not store managed credentials file",
    );
}

#[test]
fn add_codex_api_key_supports_explicit_system_keyring_backend() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");

    env.cmd()
        .args([
            "add",
            "codex",
            "work",
            "--api-key",
            VALID_CODEX_KEY,
            "--credential-backend",
            "system-keyring",
        ])
        .assert()
        .success()
        .stdout(contains("Backend"))
        .stdout(contains("system_keyring"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["codex"]["work"]["credential_backend"],
        "system_keyring"
    );
    assert!(
        !env.home_file("profiles/codex/work/auth.json").exists(),
        "keyring-backed Codex API key profile should not store managed auth.json",
    );
    env.assert_home_file_exists("profiles/codex/work/config.toml");
}

#[test]
fn add_codex_rejects_config_only_duplicate_before_writing_system_keyring() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");
    write_config_only_profile(&env, "codex", "work", "system_keyring");
    let secret_path = seed_aisw_keyring_secret(&env, "codex", "work", r#"{"token":"old"}"#);

    env.cmd()
        .args([
            "add",
            "codex",
            "work",
            "--api-key",
            VALID_CODEX_KEY,
            "--credential-backend",
            "system-keyring",
        ])
        .assert()
        .failure()
        .stderr(contains("already exists"));

    assert_eq!(
        fs::read_to_string(&secret_path).unwrap(),
        r#"{"token":"old"}"#
    );
    assert!(
        !env.home_file("profiles/codex/work").exists(),
        "config-only duplicate must not create a profile directory"
    );
}

#[test]
fn add_gemini_rejects_explicit_system_keyring_backend() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args([
            "add",
            "gemini",
            "work",
            "--api-key",
            VALID_GEMINI_KEY,
            "--credential-backend",
            "system-keyring",
        ])
        .assert()
        .failure()
        .stderr(contains("not supported for gemini"))
        .stderr(contains("file-managed"));
}

// ---- Gemini ----

#[test]
fn add_gemini_api_key_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("Tool"))
        .stdout(contains("Gemini CLI"))
        .stdout(contains("work"))
        .stdout(contains("Next"))
        .stdout(contains("aisw use gemini work"));
}

#[test]
fn add_gemini_tool_not_installed_fails() {
    TestEnv::new()
        .cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .failure()
        .stderr(contains("not installed"));
}

#[test]
fn add_duplicate_gemini_api_key_under_different_name_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success();

    let output = env.output(&["add", "gemini", "backup", "--api-key", VALID_GEMINI_KEY]);
    assert!(!output.status.success(), "duplicate add should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("API key already exists as profile 'work'"));
    assert_output_redacts_secret(&output, VALID_GEMINI_KEY);
}

#[test]
fn add_distinct_gemini_api_keys_under_different_names_succeeds() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args(["add", "gemini", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success();

    env.cmd()
        .args([
            "add",
            "gemini",
            "personal",
            "--api-key",
            VALID_GEMINI_KEY_ALT,
        ])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("personal"));
}

#[test]
fn add_gemini_oauth_succeeds_with_mocked_binary() {
    let env = TestEnv::new();
    env.add_script_tool(
        "gemini",
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then\n\
           echo 'gemini 0.9.0'\n\
           exit 0\n\
         fi\n\
         /bin/mkdir -p \"$GEMINI_CLI_HOME/.gemini\"\n\
         printf '%s' '{\"token\":\"tok\"}' > \"$GEMINI_CLI_HOME/.gemini/oauth_creds.json\"\n\
         printf '%s' '{\"account\":\"work\"}' > \"$GEMINI_CLI_HOME/.gemini/settings.json\"\n",
    );

    env.cmd()
        .args(["add", "gemini", "work"])
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["gemini"]["work"]["auth_method"],
        "o_auth"
    );
    env.assert_home_file_exists("profiles/gemini/work/oauth_creds.json");
    env.assert_home_file_exists("profiles/gemini/work/settings.json");
}

#[test]
fn add_antigravity_oauth_succeeds_with_mocked_binary() {
    let env = TestEnv::new();
    env.add_script_tool(
        "agy",
        &format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n\
               echo 'agy 1.0.0'\n\
               exit 0\n\
             fi\n\
             root=\"${{AISW_KEYRING_TEST_DIR:-$HOME/keychain}}/gemini/antigravity\"\n\
             /bin/mkdir -p \"$root\" \"$HOME/.gemini/antigravity-cli/cache\" \"$HOME/.gemini/config/projects\"\n\
             printf '%s' 'antigravity' > \"$root/account\"\n\
             printf '%s' '{{}}' > \"$HOME/.gemini/antigravity-cli/cache/projects.json\"\n\
             printf '%s' '{{}}' > \"$HOME/.gemini/config/hooks.json\"\n\
             printf '%s' '{{}}' > \"$HOME/.gemini/config/projects/repo.json\"\n\
             printf '%s' '{{\"theme\":\"terminal\"}}' > \"$HOME/.gemini/antigravity-cli/settings.json\"\n\
             printf '%s' '{ANTIGRAVITY_SECRET}' > \"$root/secret\"\n"
        ),
    );

    env.cmd()
        .args(["add", "antigravity", "work"])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("Antigravity CLI"))
        .stdout(contains("oauth_shared_live_keyring"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["antigravity"]["work"]["auth_method"],
        "o_auth"
    );
    assert_eq!(
        config["profiles"]["antigravity"]["work"]["credential_backend"],
        "file"
    );
    env.assert_home_file_exists("profiles/antigravity/work/keyring-secret.json");
    env.assert_home_file_exists("profiles/antigravity/work/keyring.json");
    env.assert_home_file_exists("profiles/antigravity/work/app/settings.json");
    env.assert_home_file_exists("profiles/antigravity/work/shared/hooks.json");
}

#[test]
fn add_antigravity_oauth_restores_prior_live_state_when_profile_save_fails() {
    let env = TestEnv::new();
    write_antigravity_live_state(&env, ANTIGRAVITY_SECRET_ALT);

    env.add_fake_tool("agy", "agy 1.0.0");
    env.cmd()
        .args(["add", "antigravity", "existing", "--from-live"])
        .assert()
        .success();

    write_antigravity_live_state(&env, ANTIGRAVITY_SECRET);
    env.add_script_tool(
        "agy",
        &format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n\
               echo 'agy 1.0.0'\n\
               exit 0\n\
             fi\n\
             root=\"${{AISW_KEYRING_TEST_DIR:-$HOME/keychain}}/gemini/antigravity\"\n\
             /bin/mkdir -p \"$root\" \"$HOME/.gemini/antigravity-cli/cache\" \"$HOME/.gemini/config/projects\"\n\
             printf '%s' 'antigravity' > \"$root/account\"\n\
             printf '%s' '{ANTIGRAVITY_SECRET_ALT}' > \"$root/secret\"\n\
             printf '%s' '{{\"theme\":\"light\"}}' > \"$HOME/.gemini/antigravity-cli/settings.json\"\n\
             printf '%s' '{{\"current\":\"other\"}}' > \"$HOME/.gemini/antigravity-cli/cache/projects.json\"\n\
             printf '%s' '{{\"hooks\":[\"x\"]}}' > \"$HOME/.gemini/config/hooks.json\"\n\
             printf '%s' '{{\"mode\":\"chat\"}}' > \"$HOME/.gemini/config/projects/repo.json\"\n"
        ),
    );

    env.cmd()
        .args(["add", "antigravity", "new-profile"])
        .assert()
        .failure()
        .stderr(contains(
            "An Antigravity OAuth profile for this account already exists as 'existing'.",
        ));

    assert_eq!(
        fs::read_to_string(antigravity_live_keyring_secret_path(&env)).unwrap(),
        ANTIGRAVITY_SECRET
    );
    assert_eq!(
        fs::read_to_string(
            env.fake_home
                .join(".gemini")
                .join("antigravity-cli")
                .join("settings.json")
        )
        .unwrap(),
        "{\"theme\":\"terminal\"}"
    );
    assert_eq!(
        fs::read_to_string(
            env.fake_home
                .join(".gemini")
                .join("antigravity-cli")
                .join("cache")
                .join("projects.json")
        )
        .unwrap(),
        "{\"current\":\"repo\"}"
    );
    assert_eq!(
        fs::read_to_string(
            env.fake_home
                .join(".gemini")
                .join("config")
                .join("hooks.json")
        )
        .unwrap(),
        "{\"hooks\":[]}"
    );
    assert_eq!(
        fs::read_to_string(
            env.fake_home
                .join(".gemini")
                .join("config")
                .join("projects")
                .join("repo.json")
        )
        .unwrap(),
        "{\"mode\":\"plan\"}"
    );
}

#[test]
fn add_antigravity_from_live_succeeds_and_activates_profile() {
    let env = TestEnv::new();
    env.add_fake_tool("agy", "agy 1.0.0");
    write_antigravity_live_state(&env, ANTIGRAVITY_SECRET);

    env.cmd()
        .args(["add", "antigravity", "work", "--from-live"])
        .assert()
        .success()
        .stdout(contains("Added profile"))
        .stdout(contains("Activation"))
        .stdout(contains("active"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["antigravity"], "work");
    assert_eq!(
        fs::read_to_string(env.home_file("profiles/antigravity/work/keyring-secret.json")).unwrap(),
        ANTIGRAVITY_SECRET
    );
}

#[test]
fn add_antigravity_from_live_supports_explicit_system_keyring_backend() {
    let env = TestEnv::new();
    env.add_fake_tool("agy", "agy 1.0.0");
    write_antigravity_live_state(&env, ANTIGRAVITY_SECRET);

    env.cmd()
        .args([
            "add",
            "antigravity",
            "work",
            "--from-live",
            "--credential-backend",
            "system-keyring",
        ])
        .assert()
        .success()
        .stdout(contains("system_keyring"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["antigravity"]["work"]["credential_backend"],
        "system_keyring"
    );
    assert!(!env
        .home_file("profiles/antigravity/work/keyring-secret.json")
        .exists());
}

#[test]
fn add_antigravity_from_live_fails_without_live_credentials() {
    let env = TestEnv::new();
    env.add_fake_tool("agy", "agy 1.0.0");

    env.cmd()
        .args(["add", "antigravity", "work", "--from-live"])
        .assert()
        .failure()
        .stderr(contains("no live Antigravity credentials found"));
}

#[test]
fn add_antigravity_rejects_api_key_auth_paths() {
    let env = TestEnv::new();
    env.add_fake_tool("agy", "agy 1.0.0");

    env.cmd()
        .args(["add", "antigravity", "work", "--api-key", VALID_GEMINI_KEY])
        .assert()
        .failure()
        .stderr(contains("OAuth-only"))
        .stderr(contains("Use 'aisw add antigravity <name>'"));

    env.cmd()
        .args(["add", "antigravity", "work", "--from-env"])
        .assert()
        .failure()
        .stderr(contains(
            "does not document API-key or environment-variable authentication",
        ));
}

#[test]
fn add_oauth_in_non_interactive_mode_fails_clearly() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    env.cmd()
        .args(["--non-interactive", "add", "claude", "work"])
        .assert()
        .failure()
        .stderr(contains("requires interactive authentication"))
        .stderr(contains("--api-key"));
}

#[test]
fn add_quiet_suppresses_human_summary_output() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 2.3.0");

    let output = env.output(&[
        "--quiet",
        "add",
        "claude",
        "work",
        "--api-key",
        VALID_CLAUDE_KEY,
    ]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "expected quiet add to be silent: {stdout}"
    );
}
