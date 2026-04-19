// Integration tests for Claude Code Keychain (secure backend) flows.
//
// Every test sets AISW_KEYRING_TEST_DIR (already wired into TestEnv::cmd()) so
// all keyring reads/writes go through the fake filesystem keychain instead of
// the real macOS Keychain or system keyring.  AISW_SECURITY_BIN is set to a
// sandboxed `security` mock so no real `security(1)` binary is invoked.
// AISW_CLAUDE_AUTH_STORAGE=keychain forces Claude to treat every operation as
// Keychain-backed even on non-macOS CI runners.
mod common;

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use common::TestEnv;
use predicates::str::contains;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn add_fake_security_tool(env: &TestEnv) {
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
         service=''\n\
         account=''\n\
         password=''\n\
         want_secret='false'\n\
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
               if [ \"$cmd\" = \"find-generic-password\" ]; then\n\
                 want_secret='true'\n\
               else\n\
                 shift\n\
                 if [ \"$#\" -gt 0 ] && [ \"${1#-}\" = \"$1\" ]; then\n\
                   password=\"$1\"\n\
                 else\n\
                   IFS= read -r password || true\n\
                   continue\n\
                 fi\n\
               fi\n\
               ;;\n\
           esac\n\
           shift\n\
         done\n\
         case \"$cmd\" in\n\
           find-generic-password)\n\
             if [ -n \"$account\" ]; then\n\
               item=\"$(item_dir \"$service\" \"$account\")\"\n\
             else\n\
               item=\"$(first_item_dir \"$service\")\" || item=''\n\
             fi\n\
             if [ -z \"$item\" ] || [ ! -f \"$item/secret\" ]; then\n\
               echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
               exit 44\n\
             fi\n\
             if [ \"$want_secret\" = 'true' ]; then\n\
               /bin/cat \"$item/secret\"\n\
             else\n\
               acct=$(/bin/cat \"$item/account\")\n\
               printf 'keychain: \"/fake/login.keychain-db\"\\n'\n\
               printf 'attributes:\\n'\n\
               printf '    \"acct\"<blob>=\"%s\"\\n' \"$acct\"\n\
             fi\n\
             ;;\n\
           add-generic-password)\n\
             item=\"$(item_dir \"$service\" \"$account\")\"\n\
             /bin/mkdir -p \"$item\"\n\
             printf '%s' \"$account\" > \"$item/account\"\n\
             printf '%s' \"$password\" > \"$item/secret\"\n\
             ;;\n\
           delete-generic-password)\n\
             item=\"$(item_dir \"$service\" \"$account\")\"\n\
             if [ -d \"$item\" ]; then\n\
               /bin/rm -rf \"$item\"\n\
             else\n\
                echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
                exit 44\n\
             fi\n\
             ;;\n\
           *)\n\
             echo \"unexpected security command: $cmd\" >&2\n\
             exit 1\n\
             ;;\n\
         esac\n",
    );
}

fn add_fake_tool_versions(env: &TestEnv) {
    env.add_fake_tool("claude", "2.1.87 (Claude Code)");
    env.add_fake_tool("codex", "codex-cli 0.117.0");
}

fn keychain_secret_path(env: &TestEnv, service: &str, account: &str) -> PathBuf {
    env.fake_home
        .join("keychain")
        .join(service)
        .join(account)
        .join("secret")
}

fn seed_keychain_item(env: &TestEnv, service: &str, account: &str, secret: &str) {
    let secret_path = keychain_secret_path(env, service, account);
    fs::create_dir_all(secret_path.parent().unwrap()).unwrap();
    fs::write(secret_path.parent().unwrap().join("account"), account).unwrap();
    fs::write(secret_path, secret).unwrap();
}

/// Paths returned by `seed_system_keyring_profile` for use in assertions.
struct SeededProfile {
    /// `aisw` keyring dir for this profile (`aisw/profile:claude:<name>`).
    stored_keychain_dir: PathBuf,
    /// `Claude Code-credentials/tester/secret` — the live credential path.
    live_keychain_path: PathBuf,
}

/// Create a SystemKeyring-backed Claude profile without going through the OAuth
/// flow.  Writes:
/// - A profile dir under AISW_HOME
/// - The aisw keyring entry (`aisw/profile:claude:<name>/secret`)
/// - A config.json entry with `credential_backend: "system_keyring"`
///
/// Returns paths useful for assertions.
fn seed_system_keyring_profile(env: &TestEnv, name: &str, secret: &str) -> SeededProfile {
    // Create the profile directory.
    let profile_dir = env.aisw_home.join("profiles").join("claude").join(name);
    fs::create_dir_all(&profile_dir).unwrap();

    // Seed the aisw keyring entry (service=aisw, account=profile:claude:<name>).
    let account = format!("profile:claude:{}", name);
    seed_keychain_item(env, "aisw", &account, secret);
    let stored_keychain_dir = env.fake_home.join("keychain").join("aisw").join(&account);

    // Write or update config.json with a SystemKeyring-backed profile entry.
    let config_path = env.aisw_home.join("config.json");
    let mut config: serde_json::Value = if config_path.exists() {
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap()
    } else {
        serde_json::json!({
            "version": 1,
            "active": {"claude": null, "codex": null, "gemini": null},
            "profiles": {"claude": {}, "codex": {}, "gemini": {}},
            "settings": {"backup_on_switch": true, "max_backups": 10}
        })
    };
    config["profiles"]["claude"][name] = serde_json::json!({
        "added_at": "2026-01-01T00:00:00Z",
        "auth_method": "o_auth",
        "credential_backend": "system_keyring",
        "label": null
    });
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    let live_keychain_path = keychain_secret_path(env, "Claude Code-credentials", "tester");

    SeededProfile {
        stored_keychain_dir,
        live_keychain_path,
    }
}

fn seed_system_keyring_codex_profile(env: &TestEnv, name: &str, secret: &str) -> PathBuf {
    let profile_dir = env.aisw_home.join("profiles").join("codex").join(name);
    fs::create_dir_all(&profile_dir).unwrap();
    fs::write(
        profile_dir.join("config.toml"),
        b"cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();

    let account = format!("profile:codex:{}", name);
    seed_keychain_item(env, "aisw", &account, secret);

    let config_path = env.aisw_home.join("config.json");
    let mut config: serde_json::Value = if config_path.exists() {
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap()
    } else {
        serde_json::json!({
            "version": 1,
            "active": {"claude": null, "codex": null, "gemini": null},
            "profiles": {"claude": {}, "codex": {}, "gemini": {}},
            "settings": {"backup_on_switch": true, "max_backups": 10}
        })
    };
    config["profiles"]["codex"][name] = serde_json::json!({
        "added_at": "2026-01-01T00:00:00Z",
        "auth_method": "o_auth",
        "credential_backend": "system_keyring",
        "label": null
    });
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    keychain_secret_path(env, "aisw", &account)
}

fn cmd_with_secure_env(env: &TestEnv) -> Command {
    let mut cmd = env.cmd();
    cmd.env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
        .env("USER", "tester");
    cmd
}

fn secure_cmd_for_tool(env: &TestEnv, tool: &str) -> Command {
    let mut cmd = cmd_with_secure_env(env);
    match tool {
        "claude" => {
            cmd.env("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        }
        "codex" => {
            cmd.env("AISW_CODEX_AUTH_STORAGE", "keychain");
        }
        _ => unreachable!(),
    }
    cmd
}

fn backup_id_for(env: &TestEnv, tool: &str, profile: &str) -> String {
    let output = cmd_with_secure_env(env)
        .args(["backup", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let entries: serde_json::Value = serde_json::from_slice(&output).unwrap();
    entries
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tool"] == tool && entry["profile"] == profile)
        .and_then(|entry| entry["backup_id"].as_str())
        .expect("expected backup entry")
        .to_owned()
}

// ---------------------------------------------------------------------------
// Claude `use` — keychain backend
// ---------------------------------------------------------------------------

/// `aisw use claude <profile>` writes credentials into the fake keychain
/// (service = "Claude Code-credentials", account = USER env var).
#[test]
fn use_claude_keychain_writes_secret_to_keychain() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    env.cmd()
        .args(["add", "claude", "work", "--api-key", "sk-ant-api03-AAAA"])
        .assert()
        .success();

    secure_cmd_for_tool(&env, "claude")
        .args(["use", "claude", "work"])
        .assert()
        .success();

    let secret = keychain_secret_path(&env, "Claude Code-credentials", "tester");
    assert!(secret.exists(), "keychain secret should be written on use");
    let stored = fs::read(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join("work")
            .join(".credentials.json"),
    )
    .unwrap();
    assert_eq!(
        fs::read(&secret).unwrap(),
        stored,
        "keychain content should match stored profile"
    );
}

/// Switching between two profiles updates the live keychain entry each time.
#[test]
fn use_claude_keychain_switches_between_profiles() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    env.cmd()
        .args(["add", "claude", "work", "--api-key", "sk-ant-api03-AAAA"])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "claude",
            "personal",
            "--api-key",
            "sk-ant-api03-BBBB",
        ])
        .assert()
        .success();

    secure_cmd_for_tool(&env, "claude")
        .args(["use", "claude", "work"])
        .assert()
        .success();
    let after_work = fs::read(keychain_secret_path(
        &env,
        "Claude Code-credentials",
        "tester",
    ))
    .unwrap();

    secure_cmd_for_tool(&env, "claude")
        .args(["use", "claude", "personal"])
        .assert()
        .success();
    let after_personal = fs::read(keychain_secret_path(
        &env,
        "Claude Code-credentials",
        "tester",
    ))
    .unwrap();

    assert_ne!(
        after_work, after_personal,
        "keychain should be updated when switching profiles"
    );

    let work_stored = fs::read(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join("work")
            .join(".credentials.json"),
    )
    .unwrap();
    let personal_stored = fs::read(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join("personal")
            .join(".credentials.json"),
    )
    .unwrap();
    assert_eq!(after_personal, personal_stored);
    assert_ne!(after_personal, work_stored);
}

// ---------------------------------------------------------------------------
// `aisw status` with keychain backend
// ---------------------------------------------------------------------------

/// `aisw status` correctly reports a Keychain-backed Claude profile as active
/// when the live keychain entry matches the stored profile.
#[test]
fn status_shows_active_when_keychain_matches_stored_profile() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    env.cmd()
        .args(["add", "claude", "work", "--api-key", "sk-ant-api03-AAAA"])
        .assert()
        .success();

    secure_cmd_for_tool(&env, "claude")
        .args(["use", "claude", "work"])
        .assert()
        .success();

    secure_cmd_for_tool(&env, "claude")
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("work"))
        .stdout(contains("Active"));
}

/// `aisw status` reports a mismatch when the keychain secret has been modified
/// externally (i.e., the live credential no longer matches the stored profile).
///
/// This requires a SystemKeyring-backed profile so that `should_skip_live_verification`
/// returns false and the live keychain is actually compared.
#[test]
fn status_detects_keychain_mismatch_when_secret_changed_externally() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    // Create a SystemKeyring-backed profile.
    let profile = seed_system_keyring_profile(&env, "work", r#"{"apiKey":"sk-ant-api03-AAAA"}"#);

    // Use the profile so the live "Claude Code-credentials" entry is written and
    // the active profile pointer is set. Status will now report "Active".
    secure_cmd_for_tool(&env, "claude")
        .args(["use", "claude", "work"])
        .assert()
        .success();

    // Tamper with the live keychain entry directly.
    fs::write(
        &profile.live_keychain_path,
        br#"{"apiKey":"tampered-externally"}"#,
    )
    .unwrap();

    secure_cmd_for_tool(&env, "claude")
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("does not match"));
}

// ---------------------------------------------------------------------------
// `aisw init` — importing existing Keychain credentials
// ---------------------------------------------------------------------------

/// `aisw init` detects an existing live keychain credential and imports it.
#[test]
fn init_imports_claude_keychain_credentials() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    // Simulate a pre-existing Claude login in the live keychain and local state dir.
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    seed_keychain_item(
        &env,
        "Claude Code-credentials",
        "tester",
        r#"{"oauthToken":"live-token"}"#,
    );

    secure_cmd_for_tool(&env, "claude")
        .args(["init", "--yes"])
        .assert()
        .success()
        .stdout(contains("Imported"));

    assert!(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join("default")
            .exists(),
        "profile 'default' should be created on import"
    );

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(config["active"]["claude"], "default");
}

/// When no live Keychain entry exists and no credentials file is present,
/// `aisw init` skips Claude without error.
#[test]
fn init_skips_claude_when_no_live_credentials() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);
    // No ~/.claude dir and no keychain entry.

    secure_cmd_for_tool(&env, "claude")
        .args(["init", "--yes"])
        .assert()
        .success();

    assert!(
        !env.aisw_home
            .join("profiles")
            .join("claude")
            .join("default")
            .exists(),
        "no profile should be created when there are no live credentials"
    );
}

// ---------------------------------------------------------------------------
// `aisw remove` — keychain secret deleted on profile removal
// ---------------------------------------------------------------------------

/// Removing a keychain-backed profile deletes its entry from the keychain.
#[test]
fn remove_claude_keychain_profile_deletes_keychain_secret() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    env.cmd()
        .args(["add", "claude", "work", "--api-key", "sk-ant-api03-AAAA"])
        .assert()
        .success();

    // Seed the aisw service entry that represents the stored profile secret.
    seed_keychain_item(
        &env,
        "aisw",
        "profile:claude:work",
        r#"{"apiKey":"sk-ant-api03-AAAA"}"#,
    );

    // Write the profile into config with SystemKeyring backend so remove knows to clean it.
    let config_path = env.aisw_home.join("config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    config["profiles"]["claude"]["work"]["credential_backend"] =
        serde_json::json!("system_keyring");
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    cmd_with_secure_env(&env)
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .success();

    let secret_path = keychain_secret_path(&env, "aisw", "profile:claude:work");
    assert!(
        !secret_path.exists(),
        "keychain secret should be deleted when profile is removed"
    );
}

// ---------------------------------------------------------------------------
// `aisw backup` / `aisw backup restore` — keychain secrets snapshotted and restored
// ---------------------------------------------------------------------------

/// `aisw backup restore` restores the keychain secret of a keychain-backed profile.
///
/// Backups are created automatically by `aisw use`. This test exercises the full
/// snapshot → corrupt → restore cycle for a SystemKeyring-backed profile.
#[test]
fn backup_restore_restores_keychain_secret() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    let profile = seed_system_keyring_profile(&env, "work", r#"{"apiKey":"sk-ant-api03-AAAA"}"#);

    // `use` auto-snapshots the profile's keychain secret before applying it to
    // the live "Claude Code-credentials" entry.
    secure_cmd_for_tool(&env, "claude")
        .args(["use", "claude", "work"])
        .assert()
        .success();

    let backup_id = backup_id_for(&env, "claude", "work");

    // Corrupt the stored profile secret in the aisw keychain.
    let stored_secret_path = keychain_secret_path(&env, "aisw", "profile:claude:work");
    fs::write(&stored_secret_path, br#"{"apiKey":"corrupted"}"#).unwrap();

    // Restore from backup — should reinstate the original secret.
    cmd_with_secure_env(&env)
        .env("AISW_CLAUDE_AUTH_STORAGE", "keychain")
        .args(["backup", "restore", &backup_id, "--yes"])
        .assert()
        .success();

    let restored = fs::read(&stored_secret_path).unwrap();
    assert_eq!(
        restored, br#"{"apiKey":"sk-ant-api03-AAAA"}"#,
        "keychain secret should be restored to backed-up value"
    );

    // The live entry should be untouched by restore (restore only updates the stored profile).
    let live = fs::read(&profile.live_keychain_path).unwrap();
    assert_eq!(
        live, br#"{"apiKey":"sk-ant-api03-AAAA"}"#,
        "live keychain entry should be unchanged by restore"
    );
}

// ---------------------------------------------------------------------------
// `aisw rename` — keychain secret moves to new account key
// ---------------------------------------------------------------------------

/// Renaming a keychain-backed profile migrates the keychain secret from the
/// old account key to the new one and removes the old entry.
#[test]
fn rename_keychain_profile_moves_secret_to_new_account() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    env.cmd()
        .args(["add", "claude", "work", "--api-key", "sk-ant-api03-AAAA"])
        .assert()
        .success();

    // Seed and register the keychain secret.
    seed_keychain_item(
        &env,
        "aisw",
        "profile:claude:work",
        r#"{"apiKey":"sk-ant-api03-AAAA"}"#,
    );

    // Patch config to SystemKeyring backend.
    let config_path = env.aisw_home.join("config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    config["profiles"]["claude"]["work"]["credential_backend"] =
        serde_json::json!("system_keyring");
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    cmd_with_secure_env(&env)
        .args(["rename", "claude", "work", "personal"])
        .assert()
        .success();

    let old_secret = keychain_secret_path(&env, "aisw", "profile:claude:work");
    let new_secret = keychain_secret_path(&env, "aisw", "profile:claude:personal");

    assert!(
        !old_secret.exists(),
        "old keychain entry should be removed after rename"
    );
    assert!(
        new_secret.exists(),
        "new keychain entry should exist after rename"
    );
    assert_eq!(
        fs::read(&new_secret).unwrap(),
        br#"{"apiKey":"sk-ant-api03-AAAA"}"#,
        "secret content should be unchanged after rename"
    );
}

// ---------------------------------------------------------------------------
// Secret redaction — keychain secret must not appear in error output
// ---------------------------------------------------------------------------

/// When `use` fails because the profile's stored keychain secret is missing,
/// no credential value should appear in stderr or stdout.
///
/// This tests the SystemKeyring path: `apply_live_credentials` reads the aisw
/// profile secret and would include it in any error that mentions the raw value.
#[test]
fn failing_claude_keychain_use_does_not_leak_secret() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    let profile = seed_system_keyring_profile(&env, "work", r#"{"apiKey":"sk-ant-api03-AAAA"}"#);

    // Delete the stored aisw profile secret so `read_stored_credentials` fails.
    // The error path should not include the credential value.
    fs::remove_dir_all(profile.stored_keychain_dir).unwrap();

    let output = secure_cmd_for_tool(&env, "claude")
        .args(["use", "claude", "work"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "use should fail with missing secret"
    );
    common::assert_output_redacts_secret(&output, "sk-ant-api03-AAAA");
}

// ---------------------------------------------------------------------------
// Codex secure backend lifecycle
// ---------------------------------------------------------------------------

#[test]
fn codex_secure_backend_lifecycle_supports_backup_restore_end_to_end() {
    let env = TestEnv::new();
    add_fake_security_tool(&env);
    add_fake_tool_versions(&env);

    let stored_secret = seed_system_keyring_codex_profile(
        &env,
        "work",
        r#"{"account":{"email":"dev@example.com"},"token":"oauth-token"}"#,
    );

    secure_cmd_for_tool(&env, "codex")
        .args(["use", "codex", "work"])
        .assert()
        .success();

    let live_auth = env.fake_home.join(".codex").join("auth.json");
    let live_config = env.fake_home.join(".codex").join("config.toml");
    assert_eq!(
        fs::read(&live_auth).unwrap(),
        br#"{"account":{"email":"dev@example.com"},"token":"oauth-token"}"#
    );
    assert!(
        fs::read_to_string(&live_config)
            .unwrap()
            .contains("cli_auth_credentials_store = \"file\""),
        "codex live config should be normalized to file-backed storage"
    );

    secure_cmd_for_tool(&env, "codex")
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("work"))
        .stdout(contains("Active"));

    cmd_with_secure_env(&env)
        .args(["rename", "codex", "work", "personal"])
        .assert()
        .success();

    let old_secret = keychain_secret_path(&env, "aisw", "profile:codex:work");
    let new_secret = keychain_secret_path(&env, "aisw", "profile:codex:personal");
    assert!(
        !old_secret.exists(),
        "old codex keyring secret should be removed"
    );
    assert!(
        new_secret.exists(),
        "renamed codex keyring secret should exist"
    );

    secure_cmd_for_tool(&env, "codex")
        .args(["use", "codex", "personal"])
        .assert()
        .success();
    let backup_id = backup_id_for(&env, "codex", "personal");

    fs::write(&new_secret, br#"{"token":"tampered"}"#).unwrap();

    cmd_with_secure_env(&env)
        .args(["backup", "restore", &backup_id, "--yes"])
        .assert()
        .success();
    assert_eq!(
        fs::read(&new_secret).unwrap(),
        br#"{"account":{"email":"dev@example.com"},"token":"oauth-token"}"#
    );

    cmd_with_secure_env(&env)
        .args(["remove", "codex", "personal", "--yes", "--force"])
        .assert()
        .success();
    assert!(
        !new_secret.exists(),
        "codex system keyring secret should be deleted on remove"
    );

    assert!(
        !stored_secret.exists(),
        "original codex keyring entry should no longer exist after rename/remove"
    );
}
