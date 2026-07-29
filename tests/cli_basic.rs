mod common;

use common::TestEnv;
use predicates::str::contains;
#[cfg(target_os = "linux")]
use std::process::Command as StdCommand;

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

#[cfg(target_os = "linux")]
#[test]
fn status_does_not_stop_when_timeout_runs_it_in_background_process_group_with_tty() {
    if !command_exists("script") || !command_exists("timeout") {
        eprintln!("skipping SIGTTOU regression: script or timeout not found");
        return;
    }

    let env = TestEnv::new();
    let status_path = env.dir.path().join("status.out");
    let exit_path = env.dir.path().join("status.exit");
    let command = format!(
        "timeout 5 {} status >{}; printf 'EXIT=%s\\n' $? >{}",
        shell_quote(&env.aisw_bin()),
        shell_quote(&status_path),
        shell_quote(&exit_path)
    );

    let output = StdCommand::new("script")
        .args(["-q", "-e", "-c", &command, "/dev/null"])
        .env("AISW_HOME", &env.aisw_home)
        .env("PATH", env.shell_path())
        .env("HOME", &env.fake_home)
        .env("AISW_KEYRING_TEST_DIR", env.fake_home.join("keychain"))
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .env_remove("AISW_SECURITY_BIN")
        .env_remove("AISW_SECURITY_KEYCHAIN")
        .env_remove("AISW_CLAUDE_AUTH_STORAGE")
        .env_remove("AISW_CODEX_AUTH_STORAGE")
        .output()
        .expect("failed to run SIGTTOU regression under script");

    assert!(
        output.status.success(),
        "script wrapper failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let exit = std::fs::read_to_string(&exit_path).expect("missing timeout exit file");
    assert_eq!(exit.trim(), "EXIT=0");
}

#[cfg(target_os = "linux")]
fn command_exists(name: &str) -> bool {
    StdCommand::new("sh")
        .args(["-c", &format!("command -v {name} >/dev/null 2>&1")])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn shell_quote(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\\''"))
}
