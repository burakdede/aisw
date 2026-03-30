// Integration tests for `aisw add` across all tools.
mod common;

use std::fs;

use common::assert_output_redacts_secret;
use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";
const VALID_CODEX_KEY_ALT: &str = "sk-codex-test-key-67890";
const VALID_GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";
const VALID_GEMINI_KEY_ALT: &str = "AIzaalt0987654321FEDCBA";

fn add_fake_codex_security_tool(env: &TestEnv) {
    env.add_script_tool(
        "security",
        "#!/bin/sh\n\
         store_root=\"${AISW_KEYRING_TEST_DIR:-$HOME/keychain}\"\n\
         item_dir() {\n\
           printf '%s/%s/%s' \"$store_root\" \"$1\" \"$2\"\n\
         }\n\
         first_item_dir() {\n\
           dir=\"$store_root/$1\"\n\
           [ -d \"$dir\" ] || return 1\n\
           for item in \"$dir\"/*; do\n\
             [ -d \"$item\" ] || continue\n\
             printf '%s' \"$item\"\n\
             return 0\n\
           done\n\
           return 1\n\
         }\n\
         cmd=\"$1\"\n\
         shift\n\
         case \"$cmd\" in\n\
           find-generic-password)\n\
             service=''\n\
             account=''\n\
             while [ \"$#\" -gt 0 ]; do\n\
               case \"$1\" in\n\
                 -s)\n\
                   shift\n\
                   service=\"$1\"\n\
                   ;;\n\
                 -a)\n\
                   shift\n\
                   account=\"$1\"\n\
                   ;;\n\
               esac\n\
               shift\n\
             done\n\
             if [ -n \"$account\" ]; then\n\
               item=\"$(item_dir \"$service\" \"$account\")\"\n\
             else\n\
               item=\"$(first_item_dir \"$service\")\" || item=''\n\
             fi\n\
             if [ -f \"$item/secret\" ]; then\n\
               /bin/cat \"$item/secret\"\n\
               exit 0\n\
             fi\n\
             echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
             exit 44\n\
             ;;\n\
           add-generic-password)\n\
             service=''\n\
             account=''\n\
             while [ \"$#\" -gt 0 ]; do\n\
               case \"$1\" in\n\
                 -s)\n\
                   shift\n\
                   service=\"$1\"\n\
                   ;;\n\
                 -a)\n\
                   shift\n\
                   account=\"$1\"\n\
                   ;;\n\
                 -w)\n\
                   shift\n\
                   item=\"$(item_dir \"$service\" \"$account\")\"\n\
                   /bin/mkdir -p \"$item\"\n\
                   printf '%s' \"$account\" > \"$item/account\"\n\
                   if [ \"$#\" -gt 0 ] && [ \"${1#-}\" = \"$1\" ]; then\n\
                     secret=\"$1\"\n\
                   else\n\
                     IFS= read -r secret || true\n\
                   fi\n\
                   printf '%s' \"$secret\" > \"$item/secret\"\n\
                   exit 0\n\
                   ;;\n\
               esac\n\
               shift\n\
             done\n\
             echo 'missing -w password' >&2\n\
             exit 1\n\
             ;;\n\
           *)\n\
             echo \"unexpected security command: $cmd\" >&2\n\
             exit 1\n\
             ;;\n\
         esac\n",
    );
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
         /bin/mkdir -p \"$CLAUDE_CONFIG_DIR\"\n\
         printf '%s' '{\"oauthToken\":\"tok\"}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n",
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
fn add_claude_oauth_succeeds_with_keychain_backed_credentials() {
    let env = TestEnv::new();
    env.add_script_tool(
        "claude",
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then\n\
           echo 'claude 2.3.0'\n\
           exit 0\n\
         fi\n\
         item=\"$AISW_KEYRING_TEST_DIR/Claude Code-credentials/${USER:-tester}\"\n\
         /bin/mkdir -p \"$item\"\n\
         printf '%s' \"${USER:-tester}\" > \"$item/account\"\n\
         printf '%s' '{\"account\":{\"email\":\"work@example.com\"}}' > \"$item/secret\"\n",
    );

    env.cmd()
        .env("AISW_CLAUDE_AUTH_STORAGE", "keychain")
        .env("USER", "tester")
        .args(["add", "claude", "work"])
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["work"]["credential_backend"],
        "system_keyring"
    );
    assert!(
        !env.home_file("profiles/claude/work/.credentials.json")
            .exists(),
        "secure Claude OAuth profile should not persist a credentials file",
    );
    assert!(
        env.fake_home
            .join("keychain/aisw/profile:claude:work/secret")
            .exists(),
        "secure Claude OAuth profile should persist its secret in the fake keyring",
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
        .env("AISW_CODEX_AUTH_STORAGE", "file")
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["profiles"]["codex"]["work"]["auth_method"], "o_auth");
    env.assert_home_file_exists("profiles/codex/work/auth.json");
    env.assert_home_file_exists("profiles/codex/work/config.toml");
}

#[test]
fn add_codex_oauth_stores_secure_backend_when_supported() {
    let env = TestEnv::new();
    add_fake_codex_security_tool(&env);
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
        .env("AISW_CODEX_AUTH_STORAGE", "keychain")
        .env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
        .assert()
        .success()
        .stdout(contains("Added profile"));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["profiles"]["codex"]["work"]["auth_method"], "o_auth");
    assert_eq!(
        config["profiles"]["codex"]["work"]["credential_backend"],
        "system_keyring"
    );
    assert!(!env.home_file("profiles/codex/work/auth.json").exists());
    env.assert_home_file_exists("profiles/codex/work/config.toml");
    assert_eq!(
        fs::read_to_string(
            env.fake_home
                .join("keychain")
                .join("aisw")
                .join("profile:codex:work")
                .join("secret"),
        )
        .unwrap(),
        "{\"token\":\"tok\",\"email\":\"work@example.com\"}"
    );
    assert!(!env
        .home_file("profiles/codex/work/.oauth-capture/auth.json")
        .exists());
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
         /bin/mkdir -p \"$HOME/.gemini\"\n\
         printf '%s' '{\"token\":\"tok\"}' > \"$HOME/.gemini/oauth_creds.json\"\n\
         printf '%s' '{\"account\":\"work\"}' > \"$HOME/.gemini/settings.json\"\n",
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
