// Integration tests for `aisw shell-hook`.
mod common;

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use common::TestEnv;
use predicates::str::contains;

const VALID_CODEX_KEY: &str = "sk-codex-test-key-12345";
const VALID_CODEX_KEY_ALT: &str = "sk-codex-test-key-67890";
const VALID_GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

fn hook_output(shell: &str) -> Vec<u8> {
    TestEnv::new()
        .cmd()
        .args(["shell-hook", shell])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone()
}

fn try_syntax_check(binary: &str, source: &[u8]) -> Option<bool> {
    let mut child = Command::new(binary)
        .arg(if binary == "fish" {
            "--no-execute"
        } else {
            "-n"
        })
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take().unwrap().write_all(source).unwrap();
    Some(child.wait().unwrap().success())
}

fn add_codex_profile(env: &TestEnv, name: &str, key: &str) {
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "codex", name, "--api-key", key])
        .assert()
        .success();
}

fn add_gemini_api_key_profile(env: &TestEnv, name: &str) {
    env.add_fake_tool("gemini", "gemini 0.9.0");
    env.cmd()
        .args(["add", "gemini", name, "--api-key", VALID_GEMINI_KEY])
        .assert()
        .success();
}

fn add_gemini_oauth_profile(env: &TestEnv, name: &str) {
    let profile_dir = env.aisw_home.join("profiles").join("gemini").join(name);
    fs::create_dir_all(&profile_dir).unwrap();
    fs::write(profile_dir.join("oauth_creds.json"), r#"{"token":"tok"}"#).unwrap();

    let config_path = env.aisw_home.join("config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["profiles"]["gemini"][name] = serde_json::json!({
        "added_at": "2026-03-30T00:00:00Z",
        "auth_method": "o_auth",
        "label": null
    });
    fs::write(config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

fn assert_real_shell_hook_behavior(shell: &str) {
    let env = TestEnv::new();
    add_codex_profile(&env, "work", VALID_CODEX_KEY);
    add_codex_profile(&env, "oss", VALID_CODEX_KEY_ALT);
    add_gemini_api_key_profile(&env, "api");
    add_gemini_oauth_profile(&env, "oauth");

    let script = format!(
        r#"
eval "$(command aisw shell-hook {shell})"
printf 'sentinel=%s\n' "${{AISW_SHELL_HOOK-__unset__}}"
aisw use codex work >/dev/null
printf 'codex_isolated=%s\n' "${{CODEX_HOME-__unset__}}"
aisw use codex oss --state-mode shared >/dev/null
printf 'codex_shared=%s\n' "${{CODEX_HOME-__unset__}}"
aisw use gemini api >/dev/null
printf 'gemini_api=%s\n' "${{GEMINI_API_KEY-__unset__}}"
aisw use gemini oauth >/dev/null
printf 'gemini_oauth=%s\n' "${{GEMINI_API_KEY-__unset__}}"
"#
    );

    let Some(output) = env.run_shell_script(shell, &script) else {
        return;
    };
    assert!(
        output.status.success(),
        "{shell} sourced hook script failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_codex_home = env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join("work")
        .display()
        .to_string();
    assert!(
        stdout.contains("sentinel=1"),
        "hook sentinel should be exported in {shell}\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("codex_isolated={expected_codex_home}")),
        "isolated codex use should export CODEX_HOME in {shell}\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("codex_shared=__unset__"),
        "shared codex use should unset CODEX_HOME in {shell}\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("gemini_api={VALID_GEMINI_KEY}")),
        "gemini API key use should export GEMINI_API_KEY in {shell}\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("gemini_oauth=__unset__"),
        "gemini OAuth use should unset GEMINI_API_KEY in {shell}\nstdout:\n{stdout}"
    );
}

#[test]
fn shell_hook_bash_exits_zero_with_expected_content() {
    TestEnv::new()
        .cmd()
        .args(["shell-hook", "bash"])
        .assert()
        .success()
        .stdout(contains("AISW_SHELL_HOOK"))
        .stdout(contains("aisw()"))
        .stdout(contains("--emit-env"));
}

#[test]
fn shell_hook_zsh_exits_zero_with_expected_content() {
    TestEnv::new()
        .cmd()
        .args(["shell-hook", "zsh"])
        .assert()
        .success()
        .stdout(contains("AISW_SHELL_HOOK"))
        .stdout(contains("aisw()"))
        .stdout(contains("--emit-env"));
}

#[test]
fn shell_hook_bash_and_zsh_output_identical() {
    let bash_out = hook_output("bash");
    let zsh_out = hook_output("zsh");
    assert_eq!(bash_out, zsh_out, "bash and zsh hooks should be identical");
}

#[test]
fn shell_hook_sentinel_is_exported() {
    TestEnv::new()
        .cmd()
        .args(["shell-hook", "bash"])
        .assert()
        .success()
        .stdout(contains("export AISW_SHELL_HOOK=1"));
}

#[test]
fn shell_hook_bash_is_valid_syntax() {
    let output = hook_output("bash");
    if let Some(ok) = try_syntax_check("bash", &output) {
        assert!(ok, "bash -n reported syntax errors in the hook");
    }
}

#[test]
fn shell_hook_zsh_is_valid_syntax() {
    let output = hook_output("zsh");
    if let Some(ok) = try_syntax_check("zsh", &output) {
        assert!(ok, "zsh -n reported syntax errors in the hook");
    }
}

#[test]
fn shell_hook_bash_updates_environment_in_real_shell() {
    assert_real_shell_hook_behavior("bash");
}

#[test]
fn shell_hook_zsh_updates_environment_in_real_shell() {
    assert_real_shell_hook_behavior("zsh");
}

#[test]
fn shell_hook_fish_exits_zero_with_expected_content() {
    TestEnv::new()
        .cmd()
        .args(["shell-hook", "fish"])
        .assert()
        .success()
        .stdout(contains("AISW_SHELL_HOOK"))
        .stdout(contains("function aisw"))
        .stdout(contains("--emit-env"))
        .stdout(contains("set -gx"));
}

#[test]
fn shell_hook_fish_sentinel_is_exported() {
    TestEnv::new()
        .cmd()
        .args(["shell-hook", "fish"])
        .assert()
        .success()
        .stdout(contains("set -gx AISW_SHELL_HOOK 1"));
}

#[test]
fn shell_hook_fish_is_valid_syntax() {
    let output = hook_output("fish");
    if let Some(ok) = try_syntax_check("fish", &output) {
        assert!(ok, "fish --no-execute reported syntax errors in the hook");
    }
}
