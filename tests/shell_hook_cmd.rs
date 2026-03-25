// Integration tests for `aisw shell-hook`.
mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::TestEnv;
use predicates::str::contains;

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
