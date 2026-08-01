//! End-to-end smoke coverage for every top-level command and subcommand.
//!
//! The audit touched shared plumbing (config writes, profile paths, tool
//! detection, JSON contracts, shell hooks). This suite exercises the full CLI
//! surface against a sandboxed home so a regression in any one command shows up
//! as a failing test rather than as a broken command nobody ran.

mod common;

use common::TestEnv;

const CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const CODEX_KEY: &str = "sk-codex-test-key-12345";
const GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

/// A sandbox with all four tool binaries faked and one profile per tool.
fn env_with_profiles() -> TestEnv {
    let env = TestEnv::new();
    for tool in ["claude", "codex", "gemini", "agy"] {
        env.add_fake_tool(tool, "1.0.0");
    }
    env.cmd()
        .args(["add", "claude", "work", "--api-key", CLAUDE_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "codex", "work", "--api-key", CODEX_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "gemini", "work", "--api-key", GEMINI_KEY])
        .assert()
        .success();
    env
}

/// Run a command, require exit 0, and return stdout.
fn ok(env: &TestEnv, args: &[&str]) -> String {
    let output = env.output(args);
    assert!(
        output.status.success(),
        "aisw {} failed (exit {:?})\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Run a command expecting valid JSON on stdout and exit 0.
fn ok_json(env: &TestEnv, args: &[&str]) -> serde_json::Value {
    let stdout = ok(env, args);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "aisw {} did not emit valid JSON: {e}\n{stdout}",
            args.join(" ")
        )
    })
}

#[test]
fn version_and_capabilities() {
    let env = TestEnv::new();
    assert!(!ok(&env, &["version"]).trim().is_empty());
    let json = ok_json(&env, &["version", "--json"]);
    assert!(json.get("version").is_some() || json["result"].get("version").is_some());

    assert!(!ok(&env, &["capabilities"]).trim().is_empty());
    ok_json(&env, &["capabilities", "--json"]);
}

#[test]
fn init_variants() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");

    ok(&env, &["init", "--yes", "--no-shell-hook"]);
    let json = ok_json(&env, &["init", "--json", "--no-shell-hook"]);
    assert_eq!(json["ok"], true);
    let detect = ok_json(
        &env,
        &["init", "--json", "--no-shell-hook", "--detect-live"],
    );
    assert_eq!(detect["ok"], true);
}

#[test]
fn add_variants_across_tools() {
    let env = TestEnv::new();
    for tool in ["claude", "codex", "gemini", "agy"] {
        env.add_fake_tool(tool, "1.0.0");
    }

    // --api-key, --label, --json
    let json = ok_json(
        &env,
        &[
            "add",
            "claude",
            "work",
            "--api-key",
            CLAUDE_KEY,
            "--label",
            "Work",
            "--json",
        ],
    );
    assert_eq!(json["ok"], true);

    // --set-active
    ok(
        &env,
        &[
            "add",
            "claude",
            "second",
            "--api-key",
            CLAUDE_KEY_ALT,
            "--set-active",
        ],
    );
    let status = ok_json(&env, &["status", "--json"]);
    let claude = status
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["tool"] == "claude")
        .unwrap();
    assert_eq!(claude["active_profile"], "second");

    // --from-env
    let out = env
        .cmd()
        .env("OPENAI_API_KEY", CODEX_KEY)
        .args(["add", "codex", "fromenv", "--from-env"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "--from-env failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // --credential-backend file is always valid.
    ok(
        &env,
        &[
            "add",
            "gemini",
            "filed",
            "--api-key",
            GEMINI_KEY,
            "--credential-backend",
            "file",
        ],
    );
}

#[test]
fn list_all_filters() {
    let env = env_with_profiles();

    ok(&env, &["list"]);
    ok_json(&env, &["list", "--json"]);
    ok(&env, &["list", "claude"]);
    ok(&env, &["list", "--tool", "codex"]);
    ok(&env, &["list", "--search", "work"]);
    ok(&env, &["list", "--sort", "name"]);
    ok(&env, &["list", "--sort", "recent"]);
    ok(&env, &["list", "--active-only"]);
}

#[test]
fn status_all_filters() {
    let env = env_with_profiles();
    ok(&env, &["use", "claude", "work"]);

    ok(&env, &["status"]);
    ok_json(&env, &["status", "--json"]);
    ok(&env, &["status", "--tool", "claude"]);
    ok(&env, &["status", "--search", "work"]);
    ok(&env, &["status", "--sort", "name"]);
    ok(&env, &["status", "--sort", "recent"]);
    ok(&env, &["status", "--active-only"]);
    ok_json(&env, &["status", "--context", "--json"]);
}

#[test]
fn use_variants() {
    let env = env_with_profiles();

    ok(&env, &["use", "claude", "work"]);
    ok_json(&env, &["use", "codex", "work", "--json"]);
    ok(&env, &["use", "claude", "work", "--state-mode", "shared"]);
    ok(&env, &["use", "claude", "work", "--state-mode", "isolated"]);
    ok(&env, &["use", "--all", "--profile", "work"]);
    ok(&env, &["use", "claude", "work", "--emit-env"]);
    ok(&env, &["use", "--all", "--profile", "work", "--emit-env"]);
}

#[test]
fn context_full_lifecycle() {
    let env = env_with_profiles();

    ok_json(
        &env,
        &[
            "context", "create", "team", "--claude", "work", "--codex", "work", "--json",
        ],
    );
    ok(&env, &["context", "list"]);
    ok_json(&env, &["context", "list", "--json"]);
    ok(&env, &["context", "list", "--search", "team"]);
    ok_json(
        &env,
        &["context", "set", "team", "--gemini", "work", "--json"],
    );
    ok_json(&env, &["context", "unset", "team", "--gemini", "--json"]);
    ok_json(&env, &["context", "use", "team", "--json"]);
    ok(&env, &["context", "use", "team", "--emit-env"]);
    ok_json(&env, &["context", "rename", "team", "squad", "--json"]);
    ok_json(&env, &["context", "remove", "squad", "--yes", "--json"]);
}

#[test]
fn rename_and_remove() {
    let env = env_with_profiles();

    ok_json(&env, &["rename", "claude", "work", "renamed", "--json"]);
    ok_json(&env, &["remove", "claude", "renamed", "--yes", "--json"]);

    // --force path: remove the active profile.
    ok(&env, &["use", "codex", "work"]);
    ok(&env, &["remove", "codex", "work", "--yes", "--force"]);
}

#[test]
fn backup_list_and_restore() {
    let env = env_with_profiles();
    // A switch creates a backup.
    ok(&env, &["use", "claude", "work"]);

    ok(&env, &["backup", "list"]);
    let json = ok_json(&env, &["backup", "list", "--json"]);
    let entries = json.as_array().expect("backup list is an array");
    assert!(!entries.is_empty(), "a switch should produce a backup");

    ok(&env, &["backup", "list", "--tool", "claude"]);
    ok(&env, &["backup", "list", "--search", "work"]);
    ok(&env, &["backup", "list", "--sort", "name"]);
    ok(&env, &["backup", "list", "--sort", "recent"]);
    ok(&env, &["backup", "list", "--active-only"]);

    let id = entries[0]["backup_id"].as_str().expect("backup id");
    ok_json(&env, &["backup", "restore", id, "--yes", "--json"]);
}

#[test]
fn doctor_verify_repair() {
    let env = env_with_profiles();

    // doctor/verify exit non-zero when something is unhealthy, so only assert
    // that they run and emit valid JSON.
    let doctor = env.output(&["doctor", "--json"]);
    serde_json::from_slice::<serde_json::Value>(&doctor.stdout).expect("doctor emits JSON");
    let verify = env.output(&["verify", "--json"]);
    serde_json::from_slice::<serde_json::Value>(&verify.stdout).expect("verify emits JSON");
    env.output(&["doctor"]);
    env.output(&["verify"]);

    ok(&env, &["repair", "--dry-run"]);
    ok_json(&env, &["repair", "--json", "--dry-run"]);
    ok_json(&env, &["repair", "--json", "--apply"]);
    ok_json(
        &env,
        &["repair", "--json", "--apply", "--fix", "home,permissions"],
    );

    // After a repair --apply, the installation is healthy.
    let doctor_after = env.output(&["doctor", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&doctor_after.stdout).unwrap();
    let failures: Vec<_> = json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["status"] == "fail")
        .collect();
    assert!(
        failures.is_empty(),
        "unexpected doctor failures: {failures:#?}"
    );
}

#[test]
fn shell_hook_every_shell() {
    let env = TestEnv::new();
    for shell in ["bash", "zsh", "fish", "pwsh"] {
        let hook = ok(&env, &["shell-hook", shell]);
        assert!(hook.contains("aisw"), "{shell} hook looks empty:\n{hook}");
    }
}

#[test]
fn workspace_and_project_bindings() {
    let env = env_with_profiles();
    ok(&env, &["context", "create", "team", "--claude", "work"]);

    ok_json(&env, &["workspace", "guard", "--mode", "warn", "--json"]);
    ok_json(&env, &["workspace", "guard", "--mode", "strict", "--json"]);
    // Return to warn so `check` cannot hard-fail the rest of the test.
    ok_json(&env, &["workspace", "guard", "--mode", "warn", "--json"]);

    ok_json(
        &env,
        &[
            "workspace",
            "bind",
            "--git-remote",
            "github.com/acme/*",
            "--context",
            "team",
            "--json",
        ],
    );
    ok_json(&env, &["workspace", "status", "--json"]);
    ok(&env, &["workspace", "status"]);
    ok_json(&env, &["workspace", "doctor", "--json"]);
    ok(&env, &["workspace", "doctor"]);
    ok(&env, &["workspace", "check"]);
    ok(&env, &["workspace", "check", "--tool", "claude"]);
    ok(&env, &["workspace", "check", "--prompt"]);
    ok_json(
        &env,
        &[
            "workspace",
            "unbind",
            "--git-remote",
            "github.com/acme/*",
            "--json",
        ],
    );

    ok(&env, &["project-bindings", "list"]);
    ok_json(&env, &["project-bindings", "list", "--json"]);
}

#[test]
fn uninstall_dry_run_then_apply() {
    let env = env_with_profiles();

    ok(&env, &["uninstall", "--dry-run"]);
    ok(&env, &["uninstall", "--dry-run", "--remove-data"]);
    assert!(env.aisw_home.exists(), "--dry-run must not delete anything");

    ok(&env, &["uninstall", "--yes"]);
    assert!(
        env.aisw_home.exists(),
        "uninstall without --remove-data keeps data"
    );

    ok(&env, &["uninstall", "--yes", "--remove-data"]);
    assert!(!env.aisw_home.exists(), "--remove-data deletes AISW_HOME");
}

/// Global flags must work on every command, not just a few.
#[test]
fn global_flags_apply_across_commands() {
    let env = env_with_profiles();

    for args in [
        vec!["--no-color", "list"],
        vec!["--quiet", "list"],
        vec!["--non-interactive", "list"],
        vec!["--no-color", "status"],
        vec!["--quiet", "status"],
        vec!["--non-interactive", "backup", "list"],
        vec!["--no-color", "context", "list"],
    ] {
        ok(&env, &args);
    }
}

/// `--json` must never write to stderr on success — machine consumers parse
/// stdout and treat stderr as a failure signal.
#[test]
fn json_mode_keeps_stderr_clean() {
    let env = env_with_profiles();
    ok(&env, &["use", "claude", "work"]);

    for args in [
        vec!["list", "--json"],
        vec!["status", "--json"],
        vec!["backup", "list", "--json"],
        vec!["context", "list", "--json"],
        vec!["project-bindings", "list", "--json"],
        vec!["workspace", "status", "--json"],
        vec!["repair", "--json", "--dry-run"],
        vec!["version", "--json"],
        vec!["capabilities", "--json"],
    ] {
        let output = env.output(&args);
        assert!(
            output.stderr.is_empty(),
            "aisw {} wrote to stderr in --json mode:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
