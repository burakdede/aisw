mod common;

use common::TestEnv;

struct Scenario {
    tool: &'static str,
    work_key: &'static str,
    personal_key: &'static str,
    env_var: &'static str,
}

fn run_json(env: &TestEnv, args: &[&str]) -> serde_json::Value {
    let output = env.output(args);
    assert!(
        output.status.success(),
        "command failed: aisw {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be valid json")
}

#[test]
fn command_edge_mode_matrix_covers_state_modes_emit_env_and_non_interactive() {
    let scenarios = [
        Scenario {
            tool: "claude",
            work_key: "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            personal_key: "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            env_var: "CLAUDE_CONFIG_DIR",
        },
        Scenario {
            tool: "codex",
            work_key: "sk-codex-test-key-12345",
            personal_key: "sk-codex-test-key-67890",
            env_var: "CODEX_HOME",
        },
    ];

    for scenario in scenarios {
        let env = TestEnv::new();
        env.add_fake_tool("claude", "2.1.87 (Claude Code)");
        env.add_fake_tool("codex", "codex-cli 0.117.0");

        env.cmd().args(["init", "--yes"]).assert().success();

        env.cmd()
            .args(["add", scenario.tool, "work", "--api-key", scenario.work_key])
            .assert()
            .success();

        env.cmd()
            .args([
                "add",
                scenario.tool,
                "personal",
                "--api-key",
                scenario.personal_key,
            ])
            .assert()
            .success();

        env.cmd()
            .args(["use", scenario.tool, "work", "--state-mode", "isolated"])
            .assert()
            .success();

        let isolated_status = run_json(&env, &["status", "--json"]);
        let row = isolated_status
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["tool"] == scenario.tool)
            .expect("status row should exist");
        assert_eq!(row["active_profile"], "work");
        assert_eq!(row["state_mode"], "isolated");

        let isolated_emit = env
            .cmd()
            .args([
                "use",
                scenario.tool,
                "personal",
                "--state-mode",
                "isolated",
                "--emit-env",
            ])
            .output()
            .unwrap();
        assert!(isolated_emit.status.success());
        let isolated_stdout = String::from_utf8_lossy(&isolated_emit.stdout);
        assert!(isolated_stdout.contains(&format!("export {}=", scenario.env_var)));

        let shared_emit = env
            .cmd()
            .args([
                "use",
                scenario.tool,
                "work",
                "--state-mode",
                "shared",
                "--emit-env",
            ])
            .output()
            .unwrap();
        assert!(shared_emit.status.success());
        let shared_stdout = String::from_utf8_lossy(&shared_emit.stdout);
        assert!(
            shared_stdout.contains(&format!("unset {}", scenario.env_var)),
            "expected shared emit-env to unset {}\nstdout:\n{}",
            scenario.env_var,
            shared_stdout
        );

        let shared_status = run_json(&env, &["status", "--json"]);
        let row = shared_status
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["tool"] == scenario.tool)
            .expect("status row should exist");
        assert_eq!(row["active_profile"], "work");
        assert_eq!(row["state_mode"], "shared");

        env.cmd()
            .args(["--non-interactive", "remove", scenario.tool, "work"])
            .assert()
            .failure();

        env.cmd()
            .args([
                "--non-interactive",
                "remove",
                scenario.tool,
                "work",
                "--force",
                "--yes",
            ])
            .assert()
            .success();

        let final_list = run_json(&env, &["list", "--json"]);
        assert_eq!(final_list[scenario.tool]["active"], serde_json::Value::Null);
        let remaining = final_list[scenario.tool]["profiles"].as_array().unwrap();
        assert!(remaining
            .iter()
            .any(|profile| profile["name"] == "personal"));
        assert!(!remaining.iter().any(|profile| profile["name"] == "work"));
    }
}
