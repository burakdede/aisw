// Integration tests for `aisw uninstall`.
mod common;

use std::fs;

use common::TestEnv;
use predicates::str::contains;

fn install_shell_hook(env: &TestEnv, shell: &str) {
    env.cmd()
        .args(["init", "--yes"])
        .env("SHELL", shell)
        .assert()
        .success();
}

fn create_managed_data(env: &TestEnv) {
    env.add_fake_tool("claude", "claude 2.3.0");
    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ])
        .assert()
        .success();
}

#[test]
fn uninstall_non_interactive_without_yes_requires_confirmation_or_dry_run() {
    let env = TestEnv::new();
    env.cmd()
        .args(["--non-interactive", "uninstall"])
        .assert()
        .failure()
        .stderr(contains("uninstall requires confirmation"))
        .stderr(contains("--dry-run"))
        .stderr(contains("--yes"));
}

#[test]
fn uninstall_dry_run_recommends_preview_and_preserves_files() {
    let env = TestEnv::new();
    install_shell_hook(&env, "/bin/zsh");
    create_managed_data(&env);

    env.cmd()
        .args(["uninstall", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("Uninstall dry run"))
        .stdout(contains("preview only"))
        .stdout(contains("run 'aisw uninstall --yes'"))
        .stdout(contains("Upstream tool directories"));

    let zshrc = env.fake_home.join(".zshrc");
    assert!(fs::read_to_string(&zshrc)
        .unwrap()
        .contains("shell-hook zsh"));
    assert!(
        env.aisw_home.exists(),
        "AISW_HOME should remain after dry-run"
    );
}

#[test]
fn uninstall_removes_all_managed_shell_hooks_and_keeps_data_by_default() {
    let env = TestEnv::new();
    install_shell_hook(&env, "/bin/zsh");
    install_shell_hook(&env, "/bin/bash");
    install_shell_hook(&env, "/usr/bin/fish");
    create_managed_data(&env);

    let zshrc = env.fake_home.join(".zshrc");
    fs::write(
        &zshrc,
        format!("export PATH=/bin\n{}", fs::read_to_string(&zshrc).unwrap()),
    )
    .unwrap();

    env.cmd()
        .args(["uninstall", "--yes"])
        .assert()
        .success()
        .stdout(contains("Uninstall complete"))
        .stdout(contains("Kept"))
        .stdout(contains("Did not modify upstream tool directories"))
        .stdout(contains("cargo uninstall aisw"))
        .stdout(contains("remove the installed aisw binary manually"));

    assert!(
        env.aisw_home.exists(),
        "AISW_HOME should be kept by default"
    );
    assert!(!fs::read_to_string(&zshrc)
        .unwrap()
        .contains("shell-hook zsh"));
    assert!(fs::read_to_string(&zshrc)
        .unwrap()
        .contains("export PATH=/bin"));

    let bashrc = if cfg!(target_os = "macos") {
        env.fake_home.join(".bash_profile")
    } else {
        env.fake_home.join(".bashrc")
    };
    assert!(!fs::read_to_string(&bashrc)
        .unwrap()
        .contains("shell-hook bash"));

    let fish = env
        .fake_home
        .join(".config")
        .join("fish")
        .join("config.fish");
    assert!(!fs::read_to_string(&fish)
        .unwrap()
        .contains("shell-hook fish"));
}

#[test]
fn uninstall_remove_data_deletes_aisw_home() {
    let env = TestEnv::new();
    install_shell_hook(&env, "/bin/zsh");
    create_managed_data(&env);

    env.cmd()
        .args(["uninstall", "--remove-data", "--yes"])
        .assert()
        .success()
        .stdout(contains("Deleted"))
        .stdout(contains("Upstream tool directories"));

    assert!(
        !env.aisw_home.exists(),
        "AISW_HOME should be deleted when --remove-data is set"
    );
    assert!(!fs::read_to_string(env.fake_home.join(".zshrc"))
        .unwrap()
        .contains("shell-hook zsh"));
}
