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

fn first_backup_id(list_output: &str) -> String {
    list_output
        .lines()
        .find_map(|line| {
            let visible = strip_ansi(line);
            let candidate = visible.split_whitespace().next()?;
            if candidate != "Backups"
                && candidate != "BACKUP"
                && !candidate.chars().all(|ch| ch == '─')
            {
                Some(candidate.to_owned())
            } else {
                None
            }
        })
        .expect("expected at least one backup entry")
}

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
        .success()
        .stdout(contains("--json"));
}

#[test]
fn backup_restore_requires_id_arg() {
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
        .stdout(contains("BACKUP ID"))
        .stdout(contains("claude"))
        .stdout(contains("work"));
}

#[test]
fn backup_list_json_output_is_valid_json_array() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    let key = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    env.cmd()
        .args(["add", "claude", "work", "--api-key", key])
        .assert()
        .success();
    env.cmd().args(["use", "claude", "work"]).assert().success();

    let output = env
        .cmd()
        .args(["backup", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("stdout is not valid JSON");
    assert!(json.is_array());
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty());
    assert_eq!(arr[0]["tool"], "claude");
    assert_eq!(arr[0]["profile"], "work");
    assert!(arr[0]["backup_id"].as_str().is_some());
}

// ── backup restore ────────────────────────────────────────────────────────────

#[test]
fn backup_restore_unknown_id_exits_nonzero() {
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

    // Capture the backup id.
    let list_out = env.cmd().args(["backup", "list"]).output().unwrap().stdout;
    let list_str = String::from_utf8_lossy(&list_out);
    let backup_id = first_backup_id(&list_str);

    // Restore using --yes skips confirmation.
    env.cmd()
        .args(["backup", "restore", "--yes", &backup_id])
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
    let backup_id = first_backup_id(&list_str);

    env.cmd()
        .args(["backup", "restore", "--yes", &backup_id])
        .assert()
        .success()
        .stdout(contains("aisw use"));
}

#[test]
fn backup_restore_prints_next_step_hint() {
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
    let backup_id = first_backup_id(&list_str);

    env.cmd()
        .args(["backup", "restore", "--yes", &backup_id])
        .assert()
        .success()
        .stdout(contains("Next"))
        .stdout(contains("aisw use claude work"));
}

#[test]
fn backup_restore_non_interactive_without_yes_fails_clearly() {
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
    let backup_id = first_backup_id(&list_str);

    env.cmd()
        .args(["--non-interactive", "backup", "restore", &backup_id])
        .assert()
        .failure()
        .stderr(contains("requires confirmation"))
        .stderr(contains("--yes"));
}

#[test]
fn backup_restore_decline_prompt_exits_nonzero() {
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
    let backup_id = first_backup_id(&list_str);

    env.cmd()
        .args(["backup", "restore", &backup_id])
        .write_stdin("n\n")
        .assert()
        .failure()
        .stderr(contains("operation cancelled by user"));
}
