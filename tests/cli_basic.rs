mod common;

use common::TestEnv;
use predicates::str::contains;

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
