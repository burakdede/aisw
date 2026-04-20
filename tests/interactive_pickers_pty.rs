mod common;

#[cfg(unix)]
mod unix {
    use std::fs;

    use super::common::TestEnv;

    const VALID_CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const VALID_CLAUDE_KEY_ALT: &str = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

    fn add_claude(env: &TestEnv, name: &str, key: &str) {
        env.add_fake_tool("claude", "claude 2.3.0");
        env.cmd()
            .args(["add", "claude", name, "--api-key", key])
            .assert()
            .success();
    }

    #[test]
    fn remove_without_profile_uses_tty_picker() {
        let env = TestEnv::new();
        add_claude(&env, "default", VALID_CLAUDE_KEY);
        add_claude(&env, "work", VALID_CLAUDE_KEY_ALT);

        // Sorted picker order is default,work; Enter chooses default.
        let result = env.run_in_pty(&["remove", "claude", "--yes", "--force"], "\n");
        assert_eq!(
            result.exit_code, 0,
            "remove picker should succeed, output:\n{}",
            result.output
        );

        let default_dir = env.home_file("profiles/claude/default");
        let work_dir = env.home_file("profiles/claude/work");
        assert!(!default_dir.exists(), "default should be removed");
        assert!(work_dir.exists(), "work should remain");
    }

    #[test]
    fn rename_without_old_name_uses_tty_picker() {
        let env = TestEnv::new();
        add_claude(&env, "default", VALID_CLAUDE_KEY);
        add_claude(&env, "work", VALID_CLAUDE_KEY_ALT);
        env.cmd().args(["use", "claude", "work"]).assert().success();

        // Active profile defaults in picker; Enter picks "work".
        let result = env.run_in_pty(&["rename", "claude", "renamed"], "\n");
        assert_eq!(
            result.exit_code, 0,
            "rename picker should succeed, output:\n{}",
            result.output
        );

        let renamed_dir = env.home_file("profiles/claude/renamed");
        let work_dir = env.home_file("profiles/claude/work");
        assert!(renamed_dir.exists(), "selected profile should be renamed");
        assert!(!work_dir.exists(), "old profile dir should be gone");

        let config: serde_json::Value =
            serde_json::from_str(&env.read_home_file("config.json")).unwrap();
        assert_eq!(config["active"]["claude"], "renamed");
    }

    #[test]
    fn backup_restore_without_id_uses_tty_picker() {
        let env = TestEnv::new();
        add_claude(&env, "work", VALID_CLAUDE_KEY);
        env.cmd().args(["use", "claude", "work"]).assert().success();

        let cred_path = env.home_file("profiles/claude/work/.credentials.json");
        let original = fs::read(&cred_path).expect("expected credentials file");
        fs::write(&cred_path, b"{\"apiKey\":\"tampered\"}").expect("failed to tamper file");

        // Enter chooses first backup id in picker.
        let result = env.run_in_pty(&["backup", "restore", "--yes"], "\n");
        assert_eq!(
            result.exit_code, 0,
            "backup restore picker should succeed, output:\n{}",
            result.output
        );

        let restored = fs::read(&cred_path).expect("expected restored credentials file");
        assert_eq!(
            restored, original,
            "credentials should be restored from backup"
        );
    }

    #[test]
    fn use_without_profile_uses_tty_picker() {
        let env = TestEnv::new();
        add_claude(&env, "default", VALID_CLAUDE_KEY);
        add_claude(&env, "work", VALID_CLAUDE_KEY_ALT);
        env.cmd().args(["use", "claude", "work"]).assert().success();

        // Active profile is pre-selected in picker; Enter selects "work".
        let result = env.run_in_pty(&["use", "claude"], "\n");
        assert_eq!(
            result.exit_code, 0,
            "use picker should succeed, output:\n{}",
            result.output
        );

        let config: serde_json::Value =
            serde_json::from_str(&env.read_home_file("config.json")).unwrap();
        assert_eq!(config["active"]["claude"], "work");
    }
}

#[cfg(not(unix))]
#[test]
fn interactive_picker_pty_placeholder_non_unix() {
    // PTY integration coverage is Unix-gated. Windows behavior is covered by
    // non-interactive deterministic tests.
}
