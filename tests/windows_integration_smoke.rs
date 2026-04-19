mod common;

#[cfg(windows)]
use common::TestEnv;

#[cfg(windows)]
fn json_output(env: &TestEnv, args: &[&str]) -> serde_json::Value {
    let out = env.output(args);
    assert!(
        out.status.success(),
        "command failed: aisw {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout should be valid json")
}

#[cfg(windows)]
#[test]
fn windows_command_surface_smoke_for_all_tools() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "2.1.87 (Claude Code)");
    env.add_fake_tool("codex", "codex-cli 0.117.0");
    env.add_fake_tool("gemini", "gemini 1.2.3");

    env.cmd().args(["init", "--yes"]).assert().success();

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
    env.cmd()
        .args([
            "add",
            "codex",
            "work",
            "--api-key",
            "sk-codex-test-key-12345",
        ])
        .assert()
        .success();
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

    env.cmd().args(["use", "claude", "work"]).assert().success();
    env.cmd().args(["use", "codex", "work"]).assert().success();
    env.cmd().args(["use", "gemini", "work"]).assert().success();

    let status = json_output(&env, &["status", "--json"]);
    let status_arr = status.as_array().expect("status json should be an array");
    assert!(status_arr.iter().any(|row| row["tool"] == "claude"
        && row["active_profile"] == "work"
        && row["binary_found"] == true));
    assert!(status_arr.iter().any(|row| row["tool"] == "codex"
        && row["active_profile"] == "work"
        && row["binary_found"] == true));
    assert!(status_arr.iter().any(|row| row["tool"] == "gemini"
        && row["active_profile"] == "work"
        && row["binary_found"] == true));

    let list = json_output(&env, &["list", "--json"]);
    assert_eq!(list["claude"]["active"], "work");
    assert_eq!(list["codex"]["active"], "work");
    assert_eq!(list["gemini"]["active"], "work");
    assert!(list["claude"]["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|profile| profile["name"] == "work"));
    assert!(list["codex"]["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|profile| profile["name"] == "work"));
    assert!(list["gemini"]["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|profile| profile["name"] == "work"));

    env.cmd()
        .args(["rename", "codex", "work", "personal"])
        .assert()
        .success();
    env.cmd()
        .args(["remove", "codex", "personal", "--yes", "--force"])
        .assert()
        .success();

    let after = json_output(&env, &["list", "--json"]);
    assert_eq!(after["codex"]["active"], serde_json::Value::Null);
    assert!(after["codex"]["profiles"].as_array().unwrap().is_empty());

    let backups = json_output(&env, &["backup", "list", "--json"]);
    assert!(
        backups.as_array().map(|v| !v.is_empty()).unwrap_or(false),
        "expected at least one backup entry after use operations"
    );
}

#[cfg(not(windows))]
#[test]
fn windows_smoke_placeholder_non_windows() {
    // This test target is intended for native Windows CI execution.
}
