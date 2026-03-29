mod common;

use common::TestEnv;
use predicates::str::contains;

fn strip_ansi(input: &str) -> String {
    let mut stripped = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }

        stripped.push(ch);
    }

    stripped
}

#[test]
fn help_flag_exits_zero() {
    TestEnv::new().cmd().arg("--help").assert().success();
}

#[test]
fn version_flag_exits_zero() {
    TestEnv::new().cmd().arg("--version").assert().success();
}

#[test]
fn unknown_subcommand_exits_nonzero() {
    TestEnv::new().cmd().arg("switch").assert().failure();
}

#[test]
fn unknown_tool_exits_nonzero() {
    TestEnv::new()
        .cmd()
        .args(["add", "chatgpt", "work"])
        .assert()
        .failure();
}

#[test]
fn list_help_mentions_tool_filter() {
    TestEnv::new()
        .cmd()
        .args(["list", "--help"])
        .assert()
        .success()
        .stdout(contains("tool"));
}

#[test]
fn add_help_mentions_api_key_flag() {
    TestEnv::new()
        .cmd()
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(contains("api-key"));
}

#[test]
fn no_color_flag_removes_ansi_from_help() {
    let output = TestEnv::new().output(&["--no-color", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.as_ref(), strip_ansi(&stdout));
}

#[test]
fn no_color_flag_removes_ansi_from_parse_errors() {
    let output = TestEnv::new().output(&["--no-color", "switch"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.as_ref(), strip_ansi(&stderr));
}

#[test]
fn no_color_env_removes_ansi_from_parse_errors() {
    let env = TestEnv::new();
    let output = env
        .cmd()
        .env("NO_COLOR", "1")
        .arg("switch")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.as_ref(), strip_ansi(&stderr));
}
