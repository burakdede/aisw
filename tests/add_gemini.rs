// Integration tests for `aisw add gemini`.
// Tests requiring the full command to be wired are expanded in AI-17.
mod common;

use common::TestEnv;
use predicates::str::contains;

#[test]
fn add_gemini_help_exits_zero() {
    TestEnv::new()
        .cmd()
        .args(["add", "gemini", "--help"])
        .assert()
        .success()
        .stdout(contains("api-key"));
}

#[test]
fn add_gemini_missing_profile_name_fails() {
    TestEnv::new()
        .cmd()
        .args(["add", "gemini"])
        .assert()
        .failure();
}
