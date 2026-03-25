mod common;

use common::TestEnv;
use predicates::str::contains;

// ── help / parse tests ────────────────────────────────────────────────────────

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

// ── backup list ───────────────────────────────────────────────────────────────

#[test]
fn backup_list_empty_shows_no_backups_message() {
    TestEnv::new()
        .cmd()
        .args(["backup", "list"])
        .assert()
        .success()
        .stdout(contains("No backups found"));
}

#[test]
fn backup_list_shows_entry_after_use() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    let key = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    // Add a Claude profile.
    env.cmd()
        .args(["add", "claude", "work", "--api-key", key])
        .assert()
        .success();

    // Switch to it — backup_on_switch is true by default, so a backup is created.
    env.cmd().args(["use", "claude", "work"]).assert().success();

    // backup list should now show an entry.
    env.cmd()
        .args(["backup", "list"])
        .assert()
        .success()
        .stdout(contains("TIMESTAMP"))
        .stdout(contains("claude"))
        .stdout(contains("work"));
}

// ── backup restore ────────────────────────────────────────────────────────────

#[test]
fn backup_restore_unknown_timestamp_exits_nonzero() {
    TestEnv::new()
        .cmd()
        .args(["backup", "restore", "--yes", "9999-99-99T00-00-00Z"])
        .assert()
        .failure()
        .stderr(contains("no backup found"));
}

#[test]
fn backup_restore_yes_restores_credentials() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    let key = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    // Add and switch — creates a backup of the original credentials.
    env.cmd()
        .args(["add", "claude", "work", "--api-key", key])
        .assert()
        .success();

    env.cmd().args(["use", "claude", "work"]).assert().success();

    // Capture the backup timestamp.
    let list_out = env.cmd().args(["backup", "list"]).output().unwrap().stdout;
    let list_str = String::from_utf8_lossy(&list_out);
    // First non-header line has the timestamp in the first column.
    let timestamp = list_str
        .lines()
        .nth(1) // skip the TIMESTAMP header
        .and_then(|l| l.split_whitespace().next())
        .expect("expected at least one backup entry");

    // Restore using --yes skips confirmation.
    env.cmd()
        .args(["backup", "restore", "--yes", timestamp])
        .assert()
        .success()
        .stdout(contains("Restored"))
        .stdout(contains("work"));
}

#[test]
fn backup_restore_prints_use_hint() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    let key = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    env.cmd()
        .args(["add", "claude", "work", "--api-key", key])
        .assert()
        .success();

    env.cmd().args(["use", "claude", "work"]).assert().success();

    let list_out = env.cmd().args(["backup", "list"]).output().unwrap().stdout;
    let list_str = String::from_utf8_lossy(&list_out);
    let timestamp = list_str
        .lines()
        .nth(1)
        .and_then(|l| l.split_whitespace().next())
        .expect("expected at least one backup entry");

    env.cmd()
        .args(["backup", "restore", "--yes", timestamp])
        .assert()
        .success()
        .stdout(contains("aisw use"));
}
