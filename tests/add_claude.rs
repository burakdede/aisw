// Integration tests for `aisw add claude`.
// Tests requiring the full command to be wired are expanded in AI-17.
mod common;

use common::TestEnv;
use predicates::str::contains;

#[test]
fn add_claude_help_exits_zero() {
    TestEnv::new()
        .cmd()
        .args(["add", "claude", "--help"])
        .assert()
        .success()
        .stdout(contains("api-key"))
        .stdout(contains("label"))
        .stdout(contains("set-active"));
}

#[test]
fn add_claude_missing_profile_name_fails() {
    TestEnv::new()
        .cmd()
        .args(["add", "claude"])
        .assert()
        .failure();
}
