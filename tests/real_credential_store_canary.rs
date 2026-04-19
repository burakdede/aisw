mod common;

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use common::TestEnv;

const KEYRING_SERVICE: &str = "aisw";

struct CanaryCleanup {
    accounts: Vec<String>,
}

impl CanaryCleanup {
    fn new() -> Self {
        Self {
            accounts: Vec::new(),
        }
    }

    fn track(&mut self, account: String) {
        self.accounts.push(account);
    }
}

impl Drop for CanaryCleanup {
    fn drop(&mut self) {
        for account in &self.accounts {
            if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, account) {
                let _ = entry.delete_credential();
            }
        }
    }
}

fn canary_suffix() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after UNIX_EPOCH")
        .as_millis();
    format!("{}-{}", std::process::id(), millis)
}

#[cfg(unix)]
fn write_fake_tool(bin_dir: &Path, name: &str, version: &str) {
    let path = bin_dir.join(name);
    fs::write(&path, format!("#!/bin/sh\necho '{}'\nexit 0\n", version)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(windows)]
fn write_fake_tool(bin_dir: &Path, name: &str, version: &str) {
    let path = bin_dir.join(format!("{name}.cmd"));
    fs::write(
        &path,
        format!("@echo off\r\necho {}\r\nexit /b 0\r\n", version),
    )
    .unwrap();
}

fn canary_cmd(env: &TestEnv, bin_dir: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::cargo_bin("aisw").expect("aisw binary not found");
    cmd.args(args)
        .env("AISW_HOME", &env.aisw_home)
        .env("HOME", &env.fake_home)
        .env("PATH", bin_dir)
        .env_remove("AISW_KEYRING_TEST_DIR")
        .env_remove("AISW_SECURITY_BIN")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME");
    cmd.output().unwrap()
}

fn assert_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{} failed\nstdout:\n{}\nstderr:\n{}",
        context,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn json_output(env: &TestEnv, bin_dir: &Path, args: &[&str]) -> serde_json::Value {
    let output = canary_cmd(env, bin_dir, args);
    assert_success(&output, &format!("aisw {}", args.join(" ")));
    serde_json::from_slice(&output.stdout).expect("stdout should be valid json")
}

fn write_config_for_profiles(env: &TestEnv, claude_profiles: &[&str], codex_profiles: &[&str]) {
    let claude_json: serde_json::Map<String, serde_json::Value> = claude_profiles
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                serde_json::json!({
                    "added_at": "2026-01-01T00:00:00Z",
                    "auth_method": if name.contains("oauth") { "o_auth" } else { "api_key" },
                    "credential_backend": "system_keyring",
                    "label": null
                }),
            )
        })
        .collect();
    let codex_json: serde_json::Map<String, serde_json::Value> = codex_profiles
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                serde_json::json!({
                    "added_at": "2026-01-01T00:00:00Z",
                    "auth_method": if name.contains("oauth") { "o_auth" } else { "api_key" },
                    "credential_backend": "system_keyring",
                    "label": null
                }),
            )
        })
        .collect();

    let config = serde_json::json!({
        "version": 1,
        "active": {"claude": null, "codex": null, "gemini": null},
        "profiles": {
            "claude": claude_json,
            "codex": codex_json,
            "gemini": {}
        },
        "settings": {"backup_on_switch": true, "max_backups": 10}
    });
    fs::write(
        env.aisw_home.join("config.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "opt-in real credential-store canary; set AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY=1"]
fn real_credential_store_canary_covers_secure_auth_modes() {
    if std::env::var("AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY").as_deref() != Ok("1") {
        eprintln!(
            "skipping real credential-store canary: set AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY=1"
        );
        return;
    }

    let env = TestEnv::new();
    let bin_dir = env.dir.path().join("canary-bin");
    fs::create_dir_all(&bin_dir).unwrap();

    write_fake_tool(&bin_dir, "claude", "2.1.87 (Claude Code)");
    write_fake_tool(&bin_dir, "codex", "codex-cli 0.117.0");

    let suffix = canary_suffix();
    let claude_oauth = format!("claude-oauth-{suffix}");
    let claude_api = format!("claude-api-{suffix}");
    let codex_oauth = format!("codex-oauth-{suffix}");
    let codex_api = format!("codex-api-{suffix}");

    fs::create_dir_all(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join(&claude_oauth),
    )
    .unwrap();
    fs::create_dir_all(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join(&claude_api),
    )
    .unwrap();

    let codex_oauth_dir = env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join(&codex_oauth);
    let codex_api_dir = env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join(&codex_api);
    fs::create_dir_all(&codex_oauth_dir).unwrap();
    fs::create_dir_all(&codex_api_dir).unwrap();
    fs::write(
        codex_oauth_dir.join("config.toml"),
        b"cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();
    fs::write(
        codex_api_dir.join("config.toml"),
        b"cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();

    write_config_for_profiles(
        &env,
        &[&claude_oauth, &claude_api],
        &[&codex_oauth, &codex_api],
    );

    let mut cleanup = CanaryCleanup::new();
    let claude_oauth_account = format!("profile:claude:{claude_oauth}");
    let claude_api_account = format!("profile:claude:{claude_api}");
    let codex_oauth_account = format!("profile:codex:{codex_oauth}");
    let codex_api_account = format!("profile:codex:{codex_api}");

    cleanup.track(claude_oauth_account.clone());
    cleanup.track(claude_api_account.clone());
    cleanup.track(codex_oauth_account.clone());
    cleanup.track(codex_api_account.clone());

    keyring::Entry::new(KEYRING_SERVICE, &claude_oauth_account)
        .unwrap()
        .set_password(r#"{"claudeAiOauth":{"accessToken":"real-claude-oauth-token"}}"#)
        .unwrap();
    keyring::Entry::new(KEYRING_SERVICE, &claude_api_account)
        .unwrap()
        .set_password(r#"{"apiKey":"sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#)
        .unwrap();
    keyring::Entry::new(KEYRING_SERVICE, &codex_oauth_account)
        .unwrap()
        .set_password(r#"{"account":{"email":"real@example.com"},"token":"codex-oauth-token"}"#)
        .unwrap();
    keyring::Entry::new(KEYRING_SERVICE, &codex_api_account)
        .unwrap()
        .set_password(r#"{"token":"sk-codex-real-canary-token"}"#)
        .unwrap();

    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "claude", &claude_oauth]),
        "claude oauth use",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "claude", &claude_api]),
        "claude api use",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "codex", &codex_oauth]),
        "codex oauth use",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "codex", &codex_api]),
        "codex api use",
    );

    let status = json_output(&env, &bin_dir, &["status", "--json"]);
    let status_rows = status.as_array().unwrap();

    let claude_row = status_rows
        .iter()
        .find(|row| row["tool"] == "claude")
        .expect("claude status row should exist");
    assert_eq!(claude_row["active_profile"], claude_api);
    assert_eq!(claude_row["credential_backend"], "system_keyring");
    assert_eq!(claude_row["credentials_present"], true);

    let codex_row = status_rows
        .iter()
        .find(|row| row["tool"] == "codex")
        .expect("codex status row should exist");
    assert_eq!(codex_row["active_profile"], codex_api);
    assert_eq!(codex_row["credential_backend"], "system_keyring");
    assert_eq!(codex_row["credentials_present"], true);

    let list = json_output(&env, &bin_dir, &["list", "--json"]);
    assert_eq!(list["claude"]["active"], claude_api);
    assert_eq!(list["codex"]["active"], codex_api);

    assert_success(
        &canary_cmd(
            &env,
            &bin_dir,
            &["remove", "claude", &claude_oauth, "--yes"],
        ),
        "claude oauth remove",
    );
    assert_success(
        &canary_cmd(
            &env,
            &bin_dir,
            &["remove", "claude", &claude_api, "--yes", "--force"],
        ),
        "claude api remove",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["remove", "codex", &codex_oauth, "--yes"]),
        "codex oauth remove",
    );
    assert_success(
        &canary_cmd(
            &env,
            &bin_dir,
            &["remove", "codex", &codex_api, "--yes", "--force"],
        ),
        "codex api remove",
    );
}
