mod common;

use std::fs;

use common::TestEnv;
use predicates::str::contains;

const VALID_CLAUDE_WORK: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const VALID_CLAUDE_PERSONAL: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const VALID_CODEX_WORK: &str = "sk-codex-test-key-12345";
const VALID_CODEX_PERSONAL: &str = "sk-codex-test-key-67890";
const VALID_GEMINI_WORK: &str = "AIzawork1234567890ABCDEF";
const VALID_GEMINI_PERSONAL: &str = "AIzauser1234567890ABCDEF";

fn setup_profiles_and_contexts(env: &TestEnv) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.add_fake_tool("codex", "codex 1.0.0");
    env.add_fake_tool("gemini", "gemini 0.9.0");

    env.cmd()
        .args([
            "add",
            "claude",
            "work-claude",
            "--api-key",
            VALID_CLAUDE_WORK,
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "claude",
            "personal-claude",
            "--api-key",
            VALID_CLAUDE_PERSONAL,
        ])
        .assert()
        .success();
    env.cmd()
        .args(["add", "codex", "work-codex", "--api-key", VALID_CODEX_WORK])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "codex",
            "personal-codex",
            "--api-key",
            VALID_CODEX_PERSONAL,
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "gemini",
            "work-gemini",
            "--api-key",
            VALID_GEMINI_WORK,
        ])
        .assert()
        .success();
    env.cmd()
        .args([
            "add",
            "gemini",
            "personal-gemini",
            "--api-key",
            VALID_GEMINI_PERSONAL,
        ])
        .assert()
        .success();

    env.cmd()
        .args([
            "context",
            "create",
            "client-acme",
            "--claude",
            "work-claude",
            "--codex",
            "work-codex",
            "--gemini",
            "work-gemini",
        ])
        .assert()
        .success();
}

fn setup_repo(env: &TestEnv, rel: &str, remote: &str) -> std::path::PathBuf {
    let repo = env.fake_home.join(rel);
    fs::create_dir_all(repo.join(".git").join("info")).unwrap();
    fs::create_dir_all(repo.join("api")).unwrap();
    fs::write(
        repo.join(".git").join("config"),
        format!("[core]\n\trepositoryformatversion = 0\n[remote \"origin\"]\n\turl = {remote}\n"),
    )
    .unwrap();
    repo
}

#[test]
fn workspace_bind_in_repo_writes_repo_local_config() {
    let env = TestEnv::new();
    setup_profiles_and_contexts(&env);
    let repo = setup_repo(&env, "clients/acme", "git@github.com:acme/api.git");

    env.cmd()
        .args([
            "workspace",
            "bind",
            repo.to_str().unwrap(),
            "--context",
            "client-acme",
        ])
        .assert()
        .success();

    let local = repo.join(".git").join("info").join("aisw.json");
    assert!(local.exists(), "repo-local workspace config should exist");
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(local).unwrap()).unwrap();
    assert_eq!(json["context"], "client-acme");
    assert!(
        !env.home_file("workspaces.json").exists(),
        "repo-local bind should not spill into the user workspace store"
    );
}

#[test]
fn workspace_status_json_reports_mismatch_for_expected_context() {
    let env = TestEnv::new();
    setup_profiles_and_contexts(&env);
    let repo = setup_repo(&env, "clients/acme", "git@github.com:acme/api.git");

    env.cmd()
        .args([
            "workspace",
            "bind",
            "--git-remote",
            "github.com/acme/*",
            "--context",
            "client-acme",
        ])
        .assert()
        .success();

    env.cmd()
        .current_dir(repo.join("api"))
        .args(["use", "claude", "personal-claude"])
        .assert()
        .success();
    env.cmd()
        .current_dir(repo.join("api"))
        .args(["use", "codex", "personal-codex"])
        .assert()
        .success();

    let output = env
        .cmd()
        .current_dir(repo.join("api"))
        .args(["workspace", "status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["matched_rule"], "git_remote:github.com/acme/*");
    assert_eq!(json["expected_context"], "client-acme");
    assert_eq!(json["status"], "mismatch");
    assert_eq!(json["recommended_command"], "aisw context use client-acme");
    assert_eq!(json["active_profiles"]["claude"], "personal-claude");
    assert_eq!(json["active_profiles"]["codex"], "personal-codex");
}

#[test]
fn workspace_bind_path_outside_repo_updates_user_store() {
    let env = TestEnv::new();
    setup_profiles_and_contexts(&env);
    let path = env.fake_home.join("scratch").join("project");
    fs::create_dir_all(&path).unwrap();

    env.cmd()
        .args([
            "workspace",
            "bind",
            path.to_str().unwrap(),
            "--context",
            "client-acme",
        ])
        .assert()
        .success();

    let json: serde_json::Value =
        serde_json::from_str(&env.read_home_file("workspaces.json")).unwrap();
    assert_eq!(json["path_rules"][0]["context"], "client-acme");
}

#[test]
fn workspace_check_warn_mode_allows_launch() {
    let env = TestEnv::new();
    setup_profiles_and_contexts(&env);
    let repo = setup_repo(&env, "clients/acme", "git@github.com:acme/api.git");

    env.cmd()
        .args([
            "workspace",
            "bind",
            repo.to_str().unwrap(),
            "--context",
            "client-acme",
        ])
        .assert()
        .success();
    env.cmd()
        .args(["use", "claude", "personal-claude"])
        .assert()
        .success();

    env.cmd()
        .current_dir(repo.join("api"))
        .args(["workspace", "check", "--tool", "claude"])
        .assert()
        .success()
        .stderr(contains("Workspace guard warning"));
}
