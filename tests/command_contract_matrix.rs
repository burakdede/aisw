mod common;

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use common::{add_fake_codex_security_tool, add_fake_security_tool, TestEnv};

#[derive(Clone, Copy)]
enum BackendKind {
    File,
    SystemKeyring,
}

#[derive(Clone, Copy)]
struct Scenario {
    tool: &'static str,
    profile: &'static str,
    auth_method: &'static str,
    backend: BackendKind,
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

fn cmd_for(env: &TestEnv, scenario: Scenario) -> Command {
    let mut cmd = env.cmd();
    if matches!(scenario.backend, BackendKind::SystemKeyring) {
        cmd.env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
            .env("USER", "tester");
        match scenario.tool {
            "claude" => {
                cmd.env("AISW_CLAUDE_AUTH_STORAGE", "keychain");
            }
            "codex" => {
                cmd.env("AISW_CODEX_AUTH_STORAGE", "keychain");
            }
            _ => {}
        }
    }
    cmd
}

fn run_json(env: &TestEnv, scenario: Scenario, args: &[&str]) -> serde_json::Value {
    let output = cmd_for(env, scenario).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "command failed: aisw {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn setup_scenario(env: &TestEnv, scenario: Scenario) {
    match (scenario.tool, scenario.auth_method, scenario.backend) {
        ("claude", "api_key", BackendKind::File) => {
            cmd_for(env, scenario)
                .args([
                    "add",
                    "claude",
                    scenario.profile,
                    "--api-key",
                    "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                ])
                .assert()
                .success();
        }
        ("codex", "api_key", BackendKind::File) => {
            cmd_for(env, scenario)
                .args([
                    "add",
                    "codex",
                    scenario.profile,
                    "--api-key",
                    "sk-codex-test-key-12345",
                ])
                .assert()
                .success();
        }
        ("gemini", "api_key", BackendKind::File) => {
            cmd_for(env, scenario)
                .args([
                    "add",
                    "gemini",
                    scenario.profile,
                    "--api-key",
                    "AIzatest1234567890ABCDEF",
                ])
                .assert()
                .success();
        }
        ("claude", "o_auth", BackendKind::SystemKeyring) => {
            fs::create_dir_all(
                env.aisw_home
                    .join("profiles")
                    .join("claude")
                    .join(scenario.profile),
            )
            .unwrap();
            seed_keychain_item(
                env,
                "aisw",
                &format!("profile:claude:{}", scenario.profile),
                r#"{"oauthToken":"claude-contract-token"}"#,
            );
            let config = serde_json::json!({
                "version": 1,
                "active": {"claude": null, "codex": null, "gemini": null},
                "profiles": {
                    "claude": {
                        scenario.profile: {
                            "added_at": "2026-01-01T00:00:00Z",
                            "auth_method": "o_auth",
                            "credential_backend": "system_keyring",
                            "label": null
                        }
                    },
                    "codex": {},
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
        ("codex", "o_auth", BackendKind::SystemKeyring) => {
            let profile_dir = env
                .aisw_home
                .join("profiles")
                .join("codex")
                .join(scenario.profile);
            fs::create_dir_all(&profile_dir).unwrap();
            fs::write(
                profile_dir.join("config.toml"),
                b"cli_auth_credentials_store = \"file\"\n",
            )
            .unwrap();
            seed_keychain_item(
                env,
                "aisw",
                &format!("profile:codex:{}", scenario.profile),
                r#"{"account":{"email":"contract@example.com"},"token":"codex-contract-token"}"#,
            );
            let config = serde_json::json!({
                "version": 1,
                "active": {"claude": null, "codex": null, "gemini": null},
                "profiles": {
                    "claude": {},
                    "codex": {
                        scenario.profile: {
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
        _ => panic!("unsupported scenario"),
    }
}

#[test]
fn command_contract_matrix_covers_tool_auth_backend_lifecycle() {
    let scenarios = [
        Scenario {
            tool: "claude",
            profile: "file-api",
            auth_method: "api_key",
            backend: BackendKind::File,
        },
        Scenario {
            tool: "codex",
            profile: "file-api",
            auth_method: "api_key",
            backend: BackendKind::File,
        },
        Scenario {
            tool: "gemini",
            profile: "file-api",
            auth_method: "api_key",
            backend: BackendKind::File,
        },
        Scenario {
            tool: "claude",
            profile: "secure-oauth",
            auth_method: "o_auth",
            backend: BackendKind::SystemKeyring,
        },
        Scenario {
            tool: "codex",
            profile: "secure-oauth",
            auth_method: "o_auth",
            backend: BackendKind::SystemKeyring,
        },
    ];

    for scenario in scenarios {
        let env = TestEnv::new();
        env.add_fake_tool("claude", "2.1.87 (Claude Code)");
        env.add_fake_tool("codex", "codex-cli 0.117.0");
        env.add_fake_tool("gemini", "gemini 1.2.3");
        if matches!(scenario.backend, BackendKind::SystemKeyring) {
            if scenario.tool == "codex" {
                add_fake_codex_security_tool(&env);
            } else {
                add_fake_security_tool(&env);
            }
        }
        setup_scenario(&env, scenario);

        cmd_for(&env, scenario)
            .args(["use", scenario.tool, scenario.profile])
            .assert()
            .success();

        let status = run_json(&env, scenario, &["status", "--json"]);
        let row = status
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["tool"] == scenario.tool)
            .expect("status row should exist");
        assert_eq!(row["active_profile"], scenario.profile);
        assert_eq!(row["binary_found"], true);

        let list = run_json(&env, scenario, &["list", "--json"]);
        assert_eq!(list[scenario.tool]["active"], scenario.profile);
        let profiles = list[scenario.tool]["profiles"].as_array().unwrap();
        assert!(profiles
            .iter()
            .any(|profile| profile["name"] == scenario.profile));

        let renamed = format!("{}-renamed", scenario.profile);
        cmd_for(&env, scenario)
            .args(["rename", scenario.tool, scenario.profile, &renamed])
            .assert()
            .success();

        let list_after_rename = run_json(&env, scenario, &["list", "--json"]);
        assert_eq!(list_after_rename[scenario.tool]["active"], renamed);
        let renamed_profiles = list_after_rename[scenario.tool]["profiles"]
            .as_array()
            .unwrap();
        assert!(renamed_profiles
            .iter()
            .any(|profile| profile["name"] == renamed));

        cmd_for(&env, scenario)
            .args(["remove", scenario.tool, &renamed, "--yes", "--force"])
            .assert()
            .success();

        let list_after_remove = run_json(&env, scenario, &["list", "--json"]);
        assert_eq!(
            list_after_remove[scenario.tool]["active"],
            serde_json::Value::Null
        );
        assert!(list_after_remove[scenario.tool]["profiles"]
            .as_array()
            .unwrap()
            .is_empty());

        cmd_for(&env, scenario)
            .args(["use", scenario.tool, &renamed])
            .assert()
            .failure();
    }
}
