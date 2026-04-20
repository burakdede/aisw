mod common;

#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
use common::TestEnv;

#[cfg(windows)]
fn keyring_secret_path(env: &TestEnv, service: &str, account: &str) -> PathBuf {
    let service_dir = env.fake_home.join("keychain").join(service);
    let entries = std::fs::read_dir(&service_dir).expect("service keyring directory should exist");
    for entry in entries {
        let entry = entry.expect("keyring entry should be readable");
        let item_dir = entry.path();
        let account_path = item_dir.join("account");
        if !account_path.exists() {
            continue;
        }
        let stored_account =
            std::fs::read_to_string(&account_path).expect("account marker should be valid UTF-8");
        if stored_account == account {
            return item_dir.join("secret");
        }
    }
    service_dir.join(account).join("secret")
}

#[cfg(windows)]
fn seed_keyring_item(env: &TestEnv, service: &str, account: &str, secret: &str) {
    let mut encoded = String::with_capacity(2 + account.len() * 2);
    encoded.push_str("h_");
    for byte in account.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    let secret_path = env
        .fake_home
        .join("keychain")
        .join(service)
        .join(encoded)
        .join("secret");
    fs::create_dir_all(
        secret_path
            .parent()
            .expect("secret path parent should exist"),
    )
    .unwrap();
    fs::write(
        secret_path
            .parent()
            .expect("secret path parent should exist")
            .join("account"),
        account,
    )
    .unwrap();
    fs::write(secret_path, secret).unwrap();
}

#[cfg(windows)]
fn secure_cmd(env: &TestEnv, tool: &str) -> assert_cmd::Command {
    let mut cmd = env.cmd();
    cmd.env("USER", "tester");
    if tool == "claude" {
        cmd.env("AISW_CLAUDE_AUTH_STORAGE", "keychain");
    }
    cmd
}

#[cfg(windows)]
fn json_output(env: &TestEnv, tool: &str, args: &[&str]) -> serde_json::Value {
    let output = secure_cmd(env, tool).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "command failed: aisw {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be valid json")
}

#[cfg(windows)]
fn seed_system_keyring_profiles(env: &TestEnv) {
    let claude_profile = env.aisw_home.join("profiles").join("claude").join("secure");
    let codex_profile = env.aisw_home.join("profiles").join("codex").join("secure");
    fs::create_dir_all(&claude_profile).unwrap();
    fs::create_dir_all(&codex_profile).unwrap();
    fs::write(
        codex_profile.join("config.toml"),
        b"cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();

    seed_keyring_item(
        env,
        "aisw",
        "profile:claude:secure",
        r#"{"claudeAiOauth":{"accessToken":"claude-secure-token"}}"#,
    );
    seed_keyring_item(
        env,
        "aisw",
        "profile:codex:secure",
        r#"{"account":{"email":"secure@example.com"},"token":"codex-secure-token"}"#,
    );

    let config = serde_json::json!({
        "version": 1,
        "active": {"claude": null, "codex": null, "gemini": null},
        "profiles": {
            "claude": {
                "secure": {
                    "added_at": "2026-01-01T00:00:00Z",
                    "auth_method": "o_auth",
                    "credential_backend": "system_keyring",
                    "label": null
                }
            },
            "codex": {
                "secure": {
                    "added_at": "2026-01-01T00:00:00Z",
                    "auth_method": "o_auth",
                    "credential_backend": "system_keyring",
                    "label": null
                }
            },
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

#[cfg(windows)]
#[test]
fn windows_system_keyring_secure_backend_parity_for_claude_and_codex() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "2.1.87 (Claude Code)");
    env.add_fake_tool("codex", "codex-cli 0.117.0");
    seed_system_keyring_profiles(&env);

    secure_cmd(&env, "claude")
        .args(["use", "claude", "secure"])
        .assert()
        .success();

    let claude_live_keyring = keyring_secret_path(&env, "Claude Code-credentials", "tester");
    assert!(claude_live_keyring.exists());

    let status_after_claude = json_output(&env, "claude", &["status", "--json"]);
    let claude_row = status_after_claude
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["tool"] == "claude")
        .expect("claude status row should exist");
    assert_eq!(claude_row["active_profile"], "secure");
    assert_eq!(claude_row["credential_backend"], "system_keyring");
    assert_eq!(claude_row["credentials_present"], true);
    assert_eq!(claude_row["active_profile_applied"], true);

    secure_cmd(&env, "codex")
        .args(["use", "codex", "secure"])
        .assert()
        .success();

    let status_after_codex = json_output(&env, "codex", &["status", "--json"]);
    let codex_status_row = status_after_codex
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["tool"] == "codex")
        .expect("codex status row should exist");
    assert_eq!(codex_status_row["active_profile"], "secure");
    assert_eq!(codex_status_row["credential_backend"], "system_keyring");
    assert_eq!(codex_status_row["credentials_present"], true);
    assert_eq!(codex_status_row["active_profile_applied"], true);

    let codex_live_auth = env.fake_home.join(".codex").join("auth.json");
    if codex_live_auth.exists() {
        let codex_live_auth_bytes = fs::read(&codex_live_auth).unwrap();
        assert_eq!(
            codex_live_auth_bytes,
            br#"{"account":{"email":"secure@example.com"},"token":"codex-secure-token"}"#
        );
    }

    let list = json_output(&env, "codex", &["list", "--json"]);
    assert_eq!(list["claude"]["active"], "secure");
    assert_eq!(list["codex"]["active"], "secure");

    secure_cmd(&env, "codex")
        .args(["rename", "codex", "secure", "secure-renamed"])
        .assert()
        .success();

    assert!(!keyring_secret_path(&env, "aisw", "profile:codex:secure").exists());
    assert!(keyring_secret_path(&env, "aisw", "profile:codex:secure-renamed").exists());

    secure_cmd(&env, "codex")
        .args(["remove", "codex", "secure-renamed", "--yes", "--force"])
        .assert()
        .success();

    assert!(!keyring_secret_path(&env, "aisw", "profile:codex:secure-renamed").exists());

    let final_status = json_output(&env, "claude", &["status", "--json"]);
    let codex_row = final_status
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["tool"] == "codex")
        .expect("codex status row should exist");
    assert_eq!(codex_row["active_profile"], serde_json::Value::Null);
}

#[cfg(not(windows))]
#[test]
fn windows_secure_backend_parity_placeholder_non_windows() {
    // This test target is intended for native Windows CI execution.
}
