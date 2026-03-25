// Integration tests for `aisw init`.
mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::TestEnv;
use predicates::str::contains;

fn run_init(env: &TestEnv) -> assert_cmd::assert::Assert {
    env.cmd().args(["init", "--yes"]).assert()
}

#[test]
fn init_creates_config_json() {
    let env = TestEnv::new();
    run_init(&env).success();
    env.assert_home_file_exists("config.json");
}

#[test]
fn init_prints_setup_complete() {
    let env = TestEnv::new();
    run_init(&env).success().stdout(contains("Setup complete."));
}

#[test]
fn init_is_idempotent_for_shell_hook() {
    let env = TestEnv::new();
    // Run twice — hook should appear exactly once in the rc file.
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/bin/zsh")
        .assert()
        .success();
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/bin/zsh")
        .assert()
        .success()
        .stdout(contains("already installed"));
}

#[test]
fn init_appends_zsh_hook_to_zshrc() {
    let env = TestEnv::new();
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/bin/zsh")
        .assert()
        .success();

    let zshrc = env.fake_home.join(".zshrc");
    assert!(zshrc.exists(), ".zshrc should be created");
    let contents = fs::read_to_string(&zshrc).unwrap();
    assert!(contents.contains("shell-hook zsh"));
}

#[test]
fn init_appends_fish_hook_to_config_fish() {
    let env = TestEnv::new();
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/usr/bin/fish")
        .assert()
        .success();

    let config_fish = env
        .fake_home
        .join(".config")
        .join("fish")
        .join("config.fish");
    assert!(config_fish.exists(), "config.fish should be created");
    let contents = fs::read_to_string(&config_fish).unwrap();
    assert!(contents.contains("shell-hook fish | source"));
}

#[test]
fn init_unknown_shell_prints_manual_instructions() {
    let env = TestEnv::new();
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", "/usr/bin/nushell")
        .assert()
        .success()
        .stdout(contains("not recognized"));
}

#[test]
fn init_imports_claude_credentials() {
    let env = TestEnv::new();
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join(".credentials.json"),
        b"{\"token\":\"oauth\"}",
    )
    .unwrap();

    run_init(&env).success().stdout(contains(
        "Imported Claude Code credentials as profile 'default'.",
    ));

    let profile_dir = env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default");
    assert!(profile_dir.join(".credentials.json").exists());

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["claude"]["default"]["auth_method"],
        "o_auth"
    );
}

#[test]
fn init_imports_codex_credentials() {
    let env = TestEnv::new();
    let codex_dir = env.fake_home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(codex_dir.join("auth.json"), b"{\"token\":\"tok\"}").unwrap();

    run_init(&env).success().stdout(contains(
        "Imported Codex CLI credentials as profile 'default'.",
    ));

    let profile_dir = env.aisw_home.join("profiles").join("codex").join("default");
    assert!(profile_dir.join("auth.json").exists());
    assert!(profile_dir.join("config.toml").exists());
    let config_toml = fs::read_to_string(profile_dir.join("config.toml")).unwrap();
    assert!(config_toml.contains("cli_auth_credentials_store = \"file\""));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["codex"]["default"]["auth_method"],
        "o_auth"
    );
}

#[test]
fn init_imports_gemini_env_credentials() {
    let env = TestEnv::new();
    let gemini_dir = env.fake_home.join(".gemini");
    fs::create_dir_all(&gemini_dir).unwrap();
    fs::write(gemini_dir.join(".env"), b"GEMINI_API_KEY=abc\n").unwrap();

    run_init(&env).success().stdout(contains(
        "Imported Gemini CLI credentials as profile 'default'.",
    ));

    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["gemini"]["default"]["auth_method"],
        "api_key"
    );
}

#[test]
fn init_imported_credentials_have_600_permissions() {
    let env = TestEnv::new();
    let claude_dir = env.fake_home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let src = claude_dir.join(".credentials.json");
    fs::write(&src, b"{\"token\":\"oauth\"}").unwrap();
    // Set broad permissions on source to verify we tighten them on copy.
    fs::set_permissions(&src, fs::Permissions::from_mode(0o644)).unwrap();

    run_init(&env).success();

    let dest = env
        .aisw_home
        .join("profiles")
        .join("claude")
        .join("default")
        .join(".credentials.json");
    env.assert_file_is_600(&dest);
}

#[test]
fn init_skips_import_when_no_credentials_found() {
    let env = TestEnv::new();
    run_init(&env)
        .success()
        .stdout(contains("no existing credentials found."));
}
