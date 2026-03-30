/// End-to-end lifecycle tests covering cross-command flows.
///
/// These tests exercise the full add → use → list → remove → backup cycle and
/// can only be written once all commands exist. Per-command tests (add_cmd.rs,
/// use_cmd.rs, etc.) cover individual command behaviour; this file covers the
/// interactions between commands.
mod common;

use common::TestEnv;
use predicates::str::contains;

const CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const CODEX_KEY: &str = "sk-codex-test-key-12345";
const CODEX_KEY_ALT: &str = "sk-codex-test-key-67890";
const GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_claude(env: &TestEnv) {
    env.add_fake_tool("claude", "claude 2.3.0");
}

fn setup_codex(env: &TestEnv) {
    env.add_fake_tool("codex", "codex 1.0.0");
}

fn setup_gemini(env: &TestEnv) {
    env.add_fake_tool("gemini", "gemini 0.9.0");
}

fn add_claude(env: &TestEnv, name: &str) {
    let key = if name == "work" {
        CLAUDE_KEY
    } else {
        CLAUDE_KEY_ALT
    };
    env.cmd()
        .args(["add", "claude", name, "--api-key", key])
        .assert()
        .success();
}

fn add_codex(env: &TestEnv, name: &str) {
    let key = if name == "work" || name == "main" {
        CODEX_KEY
    } else {
        CODEX_KEY_ALT
    };
    env.cmd()
        .args(["add", "codex", name, "--api-key", key])
        .assert()
        .success();
}

fn add_gemini(env: &TestEnv, name: &str) {
    env.cmd()
        .args(["add", "gemini", name, "--api-key", GEMINI_KEY])
        .assert()
        .success();
}

fn list_json(env: &TestEnv) -> serde_json::Value {
    let out = env
        .cmd()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).expect("list --json output is not valid JSON")
}

fn status_json(env: &TestEnv) -> serde_json::Value {
    let out = env
        .cmd()
        .args(["status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).expect("status --json output is not valid JSON")
}

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
        .expect("expected backup id")
}

// ---------------------------------------------------------------------------
// Full per-tool lifecycle: add → use → list → remove
// ---------------------------------------------------------------------------

#[test]
fn claude_full_lifecycle_add_use_list_remove() {
    let env = TestEnv::new();
    setup_claude(&env);

    // 1. add
    add_claude(&env, "work");
    let j = list_json(&env);
    assert_eq!(j.as_array().unwrap().len(), 1);
    assert_eq!(j[0]["tool"], "claude");
    assert_eq!(j[0]["profile"], "work");
    assert_eq!(j[0]["active"], false);

    // 2. use — activates and creates backup
    env.cmd().args(["use", "claude", "work"]).assert().success();
    let j = list_json(&env);
    assert_eq!(j[0]["active"], true);
    assert!(env.home_file("backups").exists());

    // 3. list shows active marker
    env.cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(contains("Claude Code"))
        .stdout(contains("work"))
        .stdout(contains("active"))
        .stdout(contains("yes"));

    // 4. remove
    env.cmd()
        .args(["remove", "claude", "work", "--yes", "--force"])
        .assert()
        .success();

    // profile gone, list empty
    let j = list_json(&env);
    assert!(j.as_array().unwrap().is_empty());
    env.cmd()
        .args(["list"])
        .assert()
        .stdout(contains("No profiles found"));
}

#[test]
fn codex_full_lifecycle_add_use_list_remove() {
    let env = TestEnv::new();
    setup_codex(&env);

    add_codex(&env, "main");

    env.cmd().args(["use", "codex", "main"]).assert().success();

    let j = list_json(&env);
    assert_eq!(j.as_array().unwrap().len(), 1);
    assert_eq!(j[0]["tool"], "codex");
    assert_eq!(j[0]["active"], true);

    env.cmd()
        .args(["remove", "codex", "main", "--yes", "--force"])
        .assert()
        .success();

    let j = list_json(&env);
    assert!(j.as_array().unwrap().is_empty());
}

#[test]
fn gemini_full_lifecycle_add_use_list_remove() {
    let env = TestEnv::new();
    setup_gemini(&env);

    add_gemini(&env, "default");

    env.cmd()
        .args(["use", "gemini", "default"])
        .assert()
        .success();

    // Gemini rewrites ~/.gemini/.env on use.
    let gemini_env = env.fake_home.join(".gemini").join(".env");
    assert!(
        gemini_env.exists(),
        "~/.gemini/.env should be written on use"
    );
    let contents = std::fs::read_to_string(&gemini_env).unwrap();
    assert!(contents.contains("GEMINI_API_KEY="));

    let j = list_json(&env);
    assert_eq!(j[0]["tool"], "gemini");
    assert_eq!(j[0]["active"], true);

    env.cmd()
        .args(["remove", "gemini", "default", "--yes", "--force"])
        .assert()
        .success();
    let j = list_json(&env);
    assert!(j.as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Two-profile switching — env var output changes on each switch
// ---------------------------------------------------------------------------

#[test]
fn switching_between_two_claude_profiles_changes_config_dir() {
    let env = TestEnv::new();
    setup_claude(&env);

    add_claude(&env, "work");
    add_claude(&env, "personal");

    // Switch to work.
    let work_env = env
        .cmd()
        .args(["use", "claude", "work", "--emit-env"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let work_str = String::from_utf8_lossy(&work_env);
    assert!(
        work_str.contains("CLAUDE_CONFIG_DIR="),
        "isolated Claude state emits CLAUDE_CONFIG_DIR"
    );

    let j = list_json(&env);
    let work = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["profile"] == "work")
        .unwrap();
    let personal = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["profile"] == "personal")
        .unwrap();
    assert_eq!(work["active"], true);
    assert_eq!(personal["active"], false);

    // Switch to personal.
    env.cmd()
        .args(["use", "claude", "personal", "--emit-env"])
        .assert()
        .success();

    let j = list_json(&env);
    let work = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["profile"] == "work")
        .unwrap();
    let personal = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["profile"] == "personal")
        .unwrap();
    assert_eq!(work["active"], false, "work should no longer be active");
    assert_eq!(personal["active"], true, "personal should now be active");
}

#[test]
fn switching_between_two_codex_profiles_updates_active() {
    let env = TestEnv::new();
    setup_codex(&env);

    add_codex(&env, "work");
    add_codex(&env, "oss");

    env.cmd()
        .args(["use", "codex", "work", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("CODEX_HOME="));

    env.cmd()
        .args(["use", "codex", "oss", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("CODEX_HOME="));

    let j = list_json(&env);
    let oss = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["profile"] == "oss")
        .unwrap();
    let work = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["profile"] == "work")
        .unwrap();
    assert_eq!(oss["active"], true);
    assert_eq!(work["active"], false);
}

#[test]
fn restore_after_remove_recreates_profile_in_config_and_can_be_used() {
    let env = TestEnv::new();
    setup_claude(&env);

    add_claude(&env, "work");
    env.cmd().args(["use", "claude", "work"]).assert().success();

    let backup_list = env
        .cmd()
        .args(["backup", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let backup_id = first_backup_id(&String::from_utf8_lossy(&backup_list)).to_owned();

    env.cmd()
        .args(["remove", "claude", "work", "--yes", "--force"])
        .assert()
        .success();

    env.cmd()
        .args(["backup", "restore", "--yes", &backup_id])
        .assert()
        .success();

    env.cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(contains("work"));
    env.cmd()
        .args(["use", "claude", "work"])
        .assert()
        .success()
        .stdout(contains("Switched profile"))
        .stdout(contains("Claude Code"))
        .stdout(contains("work"));
}

// ---------------------------------------------------------------------------
// --set-active on add — reflected immediately in list and status
// ---------------------------------------------------------------------------

#[test]
fn set_active_on_add_reflected_in_list_json() {
    let env = TestEnv::new();
    setup_claude(&env);

    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            CLAUDE_KEY,
            "--set-active",
        ])
        .assert()
        .success();

    let j = list_json(&env);
    assert_eq!(
        j[0]["active"], true,
        "--set-active should mark profile active"
    );
}

#[test]
fn set_active_on_add_reflected_in_status_json() {
    let env = TestEnv::new();
    setup_claude(&env);

    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            CLAUDE_KEY,
            "--set-active",
        ])
        .assert()
        .success();

    let j = status_json(&env);
    let claude = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["tool"] == "claude")
        .unwrap();
    assert_eq!(claude["active_profile"], "work");
}

#[test]
fn add_without_set_active_does_not_change_active_in_status() {
    let env = TestEnv::new();
    setup_claude(&env);

    // Add work with --set-active, then add personal without.
    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            CLAUDE_KEY,
            "--set-active",
        ])
        .assert()
        .success();
    env.cmd()
        .args(["add", "claude", "personal", "--api-key", CLAUDE_KEY_ALT])
        .assert()
        .success();

    let j = status_json(&env);
    let claude = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["tool"] == "claude")
        .unwrap();
    assert_eq!(
        claude["active_profile"], "work",
        "active should remain 'work'"
    );
}

// ---------------------------------------------------------------------------
// Status tracks state throughout the lifecycle
// ---------------------------------------------------------------------------

#[test]
fn status_shows_no_active_before_use_and_active_after() {
    let env = TestEnv::new();
    setup_claude(&env);

    add_claude(&env, "work");

    // Before use: profile exists, but none is active yet.
    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("Active"))
        .stdout(contains("none"))
        .stdout(contains("profiles stored, but none is active"));

    // After use: profile visible.
    env.cmd().args(["use", "claude", "work"]).assert().success();

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("State"))
        .stdout(contains("work"))
        .stdout(contains("credentials present"));

    // After remove --force: back to no active profile because nothing is stored.
    env.cmd()
        .args(["remove", "claude", "work", "--yes", "--force"])
        .assert()
        .success();

    env.cmd()
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("no active profile"));
}

// ---------------------------------------------------------------------------
// list --json valid throughout the full cycle
// ---------------------------------------------------------------------------

#[test]
fn list_json_valid_and_accurate_throughout_cycle() {
    let env = TestEnv::new();
    setup_claude(&env);
    setup_codex(&env);

    // Empty.
    let j = list_json(&env);
    assert!(j.is_array());
    assert!(j.as_array().unwrap().is_empty());

    // After adding two tools.
    add_claude(&env, "work");
    add_codex(&env, "main");

    let j = list_json(&env);
    assert_eq!(j.as_array().unwrap().len(), 2);
    assert!(j.as_array().unwrap().iter().all(|e| e["active"] == false));

    // After activating claude.
    env.cmd().args(["use", "claude", "work"]).assert().success();
    let j = list_json(&env);
    let claude = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["tool"] == "claude")
        .unwrap();
    let codex = j
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["tool"] == "codex")
        .unwrap();
    assert_eq!(claude["active"], true);
    assert_eq!(codex["active"], false);

    // After removing claude.
    env.cmd()
        .args(["remove", "claude", "work", "--yes", "--force"])
        .assert()
        .success();
    let j = list_json(&env);
    assert_eq!(j.as_array().unwrap().len(), 1);
    assert_eq!(j[0]["tool"], "codex");
}

// ---------------------------------------------------------------------------
// Backup → restore → use chain
// ---------------------------------------------------------------------------

#[test]
fn backup_restore_then_use_completes_successfully() {
    let env = TestEnv::new();
    setup_claude(&env);

    add_claude(&env, "work");

    // use creates a backup.
    env.cmd().args(["use", "claude", "work"]).assert().success();

    // Get backup id from backup list.
    let list_out = env
        .cmd()
        .args(["backup", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list_str = String::from_utf8_lossy(&list_out);
    let backup_id = first_backup_id(&list_str);

    // Restore the backup.
    env.cmd()
        .args(["backup", "restore", "--yes", &backup_id])
        .assert()
        .success()
        .stdout(contains("Restored"));

    // After restore, we should still be able to use the profile.
    env.cmd()
        .args(["use", "claude", "work", "--emit-env"])
        .assert()
        .success()
        .stdout(contains("CLAUDE_CONFIG_DIR="));
}

#[test]
fn backup_list_grows_with_each_switch() {
    let env = TestEnv::new();
    setup_claude(&env);

    add_claude(&env, "work");
    add_claude(&env, "personal");

    // Three switches → three backups.
    env.cmd().args(["use", "claude", "work"]).assert().success();
    env.cmd()
        .args(["use", "claude", "personal"])
        .assert()
        .success();
    env.cmd().args(["use", "claude", "work"]).assert().success();

    let list_out = env
        .cmd()
        .args(["backup", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list_str = String::from_utf8_lossy(&list_out);
    // Count non-header, non-empty lines. Backup ids are unique, so three
    // switches should produce three distinct backup entries.
    let backup_count = list_str
        .lines()
        .filter_map(|line| {
            let visible = strip_ansi(line);
            visible.split_whitespace().next().map(ToOwned::to_owned)
        })
        .filter(|candidate| {
            candidate != "Backups"
                && candidate != "BACKUP"
                && !candidate.chars().all(|ch| ch == '─')
        })
        .count();
    assert_eq!(backup_count, 3, "expected 3 backups, got {}", backup_count);
}

// ---------------------------------------------------------------------------
// Multi-tool state — all tools simultaneously
// ---------------------------------------------------------------------------

#[test]
fn multi_tool_state_each_tool_independent() {
    let env = TestEnv::new();
    setup_claude(&env);
    setup_codex(&env);
    setup_gemini(&env);

    add_claude(&env, "work");
    add_codex(&env, "main");
    add_gemini(&env, "default");

    // Activate each tool separately.
    env.cmd().args(["use", "claude", "work"]).assert().success();
    env.cmd().args(["use", "codex", "main"]).assert().success();
    env.cmd()
        .args(["use", "gemini", "default"])
        .assert()
        .success();

    let j = list_json(&env);
    let arr = j.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    for entry in arr {
        assert_eq!(entry["active"], true, "all three profiles should be active");
    }

    // Remove codex — claude and gemini unaffected.
    env.cmd()
        .args(["remove", "codex", "main", "--yes", "--force"])
        .assert()
        .success();

    let j = list_json(&env);
    let arr = j.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert!(
        arr.iter().all(|e| e["tool"] != "codex"),
        "codex should be gone"
    );
    assert!(
        arr.iter().all(|e| e["active"] == true),
        "claude and gemini should still be active"
    );
}

#[test]
fn remove_one_tool_profile_does_not_affect_other_tools_in_list_json() {
    let env = TestEnv::new();
    setup_claude(&env);
    setup_codex(&env);

    add_claude(&env, "work");
    add_codex(&env, "main");

    env.cmd()
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .success();

    let j = list_json(&env);
    let arr = j.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["tool"], "codex");
    assert_eq!(arr[0]["profile"], "main");
}

// ---------------------------------------------------------------------------
// Error state — commands fail gracefully after lifecycle events
// ---------------------------------------------------------------------------

#[test]
fn use_after_remove_fails_with_not_found() {
    let env = TestEnv::new();
    setup_claude(&env);

    add_claude(&env, "work");
    env.cmd()
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .success();

    env.cmd()
        .args(["use", "claude", "work", "--emit-env"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

#[test]
fn add_after_remove_creates_fresh_profile() {
    let env = TestEnv::new();
    setup_claude(&env);

    add_claude(&env, "work");
    env.cmd()
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .success();

    // Re-add — should succeed since profile no longer exists.
    add_claude(&env, "work");
    env.cmd().args(["use", "claude", "work"]).assert().success();

    let j = list_json(&env);
    assert_eq!(j[0]["active"], true);
}

#[test]
fn list_json_never_includes_removed_tool_profiles() {
    let env = TestEnv::new();
    setup_claude(&env);

    add_claude(&env, "work");
    add_claude(&env, "personal");

    env.cmd()
        .args(["remove", "claude", "work", "--yes"])
        .assert()
        .success();

    let j = list_json(&env);
    let arr = j.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(
        arr.iter().all(|e| e["profile"] != "work"),
        "removed profile must not appear in list --json"
    );
    assert!(arr.iter().any(|e| e["profile"] == "personal"));
}
