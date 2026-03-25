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
    let mut child = Command::new("bash")
        .arg("-n")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("bash not found");
    child.stdin.take().unwrap().write_all(&output).unwrap();
    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "bash -n reported syntax errors in the hook"
    );
}

#[test]
fn shell_hook_zsh_is_valid_syntax() {
    // zsh -n may not be available everywhere; skip gracefully if not found.
    let Ok(mut child) = Command::new("zsh")
        .arg("-n")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return; // zsh not installed, skip
    };
    let output = hook_output("zsh");
    child.stdin.take().unwrap().write_all(&output).unwrap();
    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "zsh -n reported syntax errors in the hook"
    );
}
