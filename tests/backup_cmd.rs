mod common;

use common::TestEnv;
use predicates::str::contains;

#[test]
fn backup_list_nonexistent_timestamp_exits_nonzero() {
    // Restore with a fake timestamp should fail gracefully once the command is wired.
    // For now, the dispatch is a stub — this test will be expanded in AI-25.
    let _ = TestEnv::new();
}

#[test]
fn backup_help_exits_zero() {
    TestEnv::new()
        .cmd()
        .args(["backup", "--help"])
        .assert()
        .success()
        .stdout(contains("list"))
        .stdout(contains("restore"));
}

#[test]
fn backup_list_help_exits_zero() {
    TestEnv::new()
        .cmd()
        .args(["backup", "list", "--help"])
        .assert()
        .success();
}

#[test]
fn backup_restore_requires_timestamp_arg() {
    TestEnv::new()
        .cmd()
        .args(["backup", "restore"])
        .assert()
        .failure();
}
