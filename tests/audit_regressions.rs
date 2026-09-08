//! Regression tests for defects found during the command-by-command audit.
//!
//! Each test pins a specific failure that shipped previously, so a future
//! refactor cannot silently reintroduce it.

mod common;

use common::TestEnv;

const CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Gemini stores API keys as `GEMINI_API_KEY=<key>` in a `.env` file the CLI
/// sources. Validation only rejected empty keys, so a key containing a newline
/// injected additional environment variables into that file.
#[test]
fn api_keys_with_control_characters_are_rejected() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 1.0.0");
    env.add_fake_tool("claude", "claude 1.0.0");
    env.add_fake_tool("codex", "codex 1.0.0");

    let injected = "AIzaLegitLooking123\nGOOGLE_CLOUD_PROJECT=attacker-project";
    let output = env
        .cmd()
        .args(["add", "gemini", "work", "--api-key", injected])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "a key containing a newline must be rejected"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("control character"),
        "stderr should explain the rejection: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let env_file = env
        .aisw_home
        .join("profiles")
        .join("gemini")
        .join("work")
        .join(".env");
    assert!(
        !env_file.exists(),
        "no profile should be written for a rejected key"
    );

    // Every tool applies the same rule.
    for (tool, key) in [
        ("claude", "sk-ant-api03-AAAA\nFOO=bar"),
        ("codex", "sk-codex-AAAA\rFOO=bar"),
    ] {
        let output = env
            .cmd()
            .args(["add", tool, "injected", "--api-key", key])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{tool} must reject a key containing a control character"
        );
    }
}

/// The normal case must keep working — this rule rejects control characters
/// only, not ordinary key charsets.
#[test]
fn ordinary_api_keys_are_still_accepted() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 1.0.0");
    env.cmd()
        .args([
            "add",
            "gemini",
            "work",
            "--api-key",
            "AIzatest1234567890ABCDEF",
        ])
        .assert()
        .success();

    let env_file = env
        .aisw_home
        .join("profiles")
        .join("gemini")
        .join("work")
        .join(".env");
    let contents = std::fs::read_to_string(env_file).unwrap();
    assert_eq!(contents, "GEMINI_API_KEY=AIzatest1234567890ABCDEF\n");
}

fn config_with_dangling_active() -> &'static str {
    r#"{
  "version": 2,
  "active": {"claude": "ghost", "codex": null, "gemini": null, "antigravity": null},
  "profiles": {"claude": {}, "codex": {}, "gemini": {}, "antigravity": {}},
  "contexts": {},
  "settings": {"backup_on_switch": true, "max_backups": 10}
}"#
}

/// `active` naming a profile with no config entry used to index a `HashMap`
/// directly and abort the process with "no entry found for key".
#[test]
fn status_reports_dangling_active_profile_instead_of_panicking() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    std::fs::write(
        env.aisw_home.join("config.json"),
        config_with_dangling_active(),
    )
    .unwrap();

    let output = env.output(&["status"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "status must not panic on a dangling active profile:\n{stderr}"
    );
    assert!(output.status.success(), "status should still exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("missing from aisw config"),
        "status should explain the dangling active profile:\n{stdout}"
    );
}

#[test]
fn status_json_reports_dangling_active_profile_instead_of_panicking() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    std::fs::write(
        env.aisw_home.join("config.json"),
        config_with_dangling_active(),
    )
    .unwrap();

    let output = env.output(&["status", "--json"]);
    assert!(output.status.success(), "status --json should exit 0");

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let claude = json
        .as_array()
        .expect("array")
        .iter()
        .find(|entry| entry["tool"] == "claude")
        .expect("claude entry");
    assert_eq!(claude["active_profile"], "ghost");
    assert_eq!(claude["credentials_present"], false);
}

/// `remove` used to snapshot, delete the keyring secret, and delete the profile
/// directory *before* the config write rejected the removal, destroying
/// credentials for a profile it then refused to remove.
#[test]
fn remove_rejects_context_referenced_profile_before_deleting_anything() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    env.cmd()
        .args(["add", "claude", "work", "--api-key", CLAUDE_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["context", "create", "team", "--claude", "work"])
        .assert()
        .success();

    let output = env.output(&["remove", "claude", "work", "--yes"]);
    assert!(
        !output.status.success(),
        "remove must fail while a context references the profile"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("referenced by contexts"),
        "error should name the blocking context:\n{stderr}"
    );

    let profile_dir = env.aisw_home.join("profiles").join("claude").join("work");
    assert!(
        profile_dir.is_dir(),
        "profile directory must survive a rejected remove"
    );
    assert!(
        profile_dir.join(".credentials.json").is_file(),
        "credentials must survive a rejected remove"
    );

    // The profile is still fully usable afterwards.
    env.cmd().args(["use", "claude", "work"]).assert().success();
}

/// Removing the active profile must clear `active` atomically with the profile
/// deletion, so config never names a profile that no longer exists.
#[test]
fn removing_active_profile_leaves_no_dangling_active_entry() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    env.cmd()
        .args(["add", "claude", "work", "--api-key", CLAUDE_KEY])
        .assert()
        .success();
    env.cmd().args(["use", "claude", "work"]).assert().success();

    env.cmd()
        .args(["remove", "claude", "work", "--yes", "--force"])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.aisw_home.join("config.json")).unwrap())
            .unwrap();
    assert_eq!(config["active"]["claude"], serde_json::Value::Null);
    assert!(config["profiles"]["claude"]
        .as_object()
        .expect("object")
        .is_empty());

    // And status stays healthy.
    let output = env.output(&["status"]);
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

/// Antigravity is a first-class tool, so its binary must be guarded by the
/// shell hook like every other tool.
#[test]
fn shell_hooks_guard_the_antigravity_binary() {
    let env = TestEnv::new();
    for shell in ["bash", "zsh", "fish", "pwsh"] {
        let output = env.output(&["shell-hook", shell]);
        assert!(output.status.success(), "shell-hook {shell} should succeed");
        let hook = String::from_utf8_lossy(&output.stdout);
        assert!(
            hook.contains("workspace check --tool antigravity"),
            "{shell} hook must guard antigravity:\n{hook}"
        );
        assert!(
            hook.contains("agy"),
            "{shell} hook must wrap the agy binary:\n{hook}"
        );
    }
}

/// `workspace status --json` omitted antigravity from `active_profiles`, so a
/// GUI reading the contract could not see the Antigravity account.
#[test]
fn workspace_status_json_includes_every_tool() {
    let env = TestEnv::new();
    let output = env.output(&["workspace", "status", "--json"]);
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let active = json["active_profiles"].as_object().expect("object");
    for tool in ["claude", "codex", "gemini", "antigravity"] {
        assert!(
            active.contains_key(tool),
            "active_profiles is missing '{tool}': {active:?}"
        );
    }
}

/// `status --context --json` omitted antigravity from the mapped profiles.
#[test]
fn status_context_json_includes_every_tool() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    env.add_fake_tool("agy", "agy 1.0.0");
    env.cmd()
        .args(["add", "claude", "work", "--api-key", CLAUDE_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["context", "create", "team", "--claude", "work"])
        .assert()
        .success();
    env.cmd().args(["use", "claude", "work"]).assert().success();

    let output = env.output(&["status", "--context", "--json"]);
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let profiles = json["context"]["profiles"]
        .as_object()
        .expect("mapped profiles object");
    for tool in ["claude", "codex", "gemini", "antigravity"] {
        assert!(
            profiles.contains_key(tool),
            "context profiles is missing '{tool}': {profiles:?}"
        );
    }
    assert_eq!(profiles["claude"], "work");
}

/// `uninstall --remove-data` deleted AISW_HOME but stranded credentials in the
/// OS keyring forever.
#[test]
#[cfg(unix)]
fn uninstall_remove_data_purges_system_keyring_secrets() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    env.cmd()
        .args([
            "add",
            "claude",
            "work",
            "--api-key",
            CLAUDE_KEY,
            "--credential-backend",
            "system-keyring",
        ])
        .assert()
        .success();

    let keychain = env.fake_home.join("keychain");
    assert!(
        secret_files(&keychain) > 0,
        "test keyring should hold the profile secret before uninstall"
    );

    env.cmd()
        .args(["uninstall", "--remove-data", "--yes"])
        .assert()
        .success();

    assert!(!env.aisw_home.exists(), "AISW_HOME should be deleted");
    assert_eq!(
        secret_files(&keychain),
        0,
        "keyring secrets must not outlive --remove-data"
    );
}

/// Count stored secrets in the fake keyring tree used by the test harness.
#[cfg(unix)]
fn secret_files(root: &std::path::Path) -> usize {
    fn walk(path: &std::path::Path, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, count);
            } else if path.file_name().is_some_and(|name| name == "secret") {
                *count += 1;
            }
        }
    }

    let mut count = 0;
    walk(root, &mut count);
    count
}

const CODEX_KEY: &str = "sk-codex-test-key-12345";
const GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

/// `doctor` looked for one hardcoded credential filename per tool. Gemini never
/// writes that name, so every valid Gemini profile produced a hard failure and
/// `aisw doctor` exited 1 — which also dragged `aisw verify` down with it.
#[test]
fn doctor_passes_for_a_healthy_profile_of_every_tool() {
    let env = TestEnv::new();
    for tool in ["claude", "codex", "gemini", "agy"] {
        env.add_fake_tool(tool, "1.0.0");
    }
    env.cmd().args(["init", "--yes"]).assert().success();
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

    let output = env.output(&["doctor", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let failures: Vec<&serde_json::Value> = json["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .filter(|check| check["status"] == "fail")
        .collect();
    assert!(
        failures.is_empty(),
        "doctor should not fail for healthy profiles: {failures:#?}"
    );
    assert!(
        output.status.success(),
        "doctor should exit 0 for healthy profiles"
    );
}

#[test]
fn doctor_reports_future_config_schema_in_json() {
    let env = TestEnv::new();
    std::fs::write(
        env.aisw_home.join("config.json"),
        r#"{"version":99,"profiles":{}}"#,
    )
    .unwrap();

    let output = env.output(&["doctor", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let config_check = json["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .find(|check| check["name"] == "config/json")
        .expect("config check");

    assert_eq!(config_check["status"], "fail");
    assert!(config_check["detail"]
        .as_str()
        .expect("config detail")
        .contains("unsupported schema v99"));
    assert!(!output.status.success());
}

/// The permission check must still catch a genuinely world-readable credential.
#[test]
#[cfg(unix)]
fn doctor_still_fails_on_broad_credential_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 1.0.0");
    env.cmd()
        .args(["add", "gemini", "work", "--api-key", GEMINI_KEY])
        .assert()
        .success();

    let env_file = env
        .aisw_home
        .join("profiles")
        .join("gemini")
        .join("work")
        .join(".env");
    std::fs::set_permissions(&env_file, std::fs::Permissions::from_mode(0o644)).unwrap();

    let output = env.output(&["doctor", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let check = json["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .find(|check| check["name"] == "permissions/gemini/work")
        .expect("gemini permission check");
    assert_eq!(check["status"], "fail", "broad permissions must fail");
    assert!(!output.status.success(), "doctor should exit non-zero");
}

/// `use --all` dropped `--emit-env`, so the shell hook's
/// `aisw use --all --profile X --emit-env` performed the full switch instead of
/// printing exports — and then the hook ran the switch a second time.
#[test]
fn use_all_honors_emit_env() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "claude", "work", "--api-key", CLAUDE_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "codex", "work", "--api-key", CODEX_KEY])
        .assert()
        .success();

    let output = env.output(&["use", "--all", "--profile", "work", "--emit-env"]);
    assert!(
        output.status.success(),
        "use --all --emit-env should succeed"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("export ") || stdout.contains("set -gx "),
        "--emit-env must print shell exports, got:\n{stdout}"
    );
    // stdout is eval'd by the shell hook, so it must contain nothing else.
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        assert!(
            line.starts_with("export ")
                || line.starts_with("unset ")
                || line.starts_with("set -gx ")
                || line.starts_with("set -e "),
            "non-shell line would be eval'd by the hook: {line}"
        );
    }

    // The switch is still recorded, exactly once.
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.aisw_home.join("config.json")).unwrap())
            .unwrap();
    assert_eq!(config["active"]["claude"], "work");
    assert_eq!(config["active"]["codex"], "work");
}

/// `use --all` dropped `--state-mode`, silently ignoring the flag.
#[test]
fn use_all_honors_state_mode() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "codex", "work", "--api-key", CODEX_KEY])
        .assert()
        .success();

    env.cmd()
        .args([
            "use",
            "--all",
            "--profile",
            "work",
            "--state-mode",
            "shared",
        ])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.aisw_home.join("config.json")).unwrap())
            .unwrap();
    assert_eq!(
        config["settings"]["codex"]["state_mode"], "shared",
        "--state-mode must reach the tools that support it"
    );
}

/// A tool that was attempted and failed must not leave the exit code at 0.
#[test]
fn use_all_exits_non_zero_when_a_tool_switch_fails() {
    let env = TestEnv::new();
    env.add_fake_tool("claude", "claude 1.0.0");
    env.add_fake_tool("codex", "codex 1.0.0");
    env.cmd()
        .args(["add", "claude", "work", "--api-key", CLAUDE_KEY])
        .assert()
        .success();
    env.cmd()
        .args(["add", "codex", "work", "--api-key", CODEX_KEY])
        .assert()
        .success();

    // Break just the codex profile's stored credentials.
    std::fs::remove_dir_all(env.aisw_home.join("profiles").join("codex").join("work")).unwrap();

    let output = env.output(&["use", "--all", "--profile", "work"]);
    assert!(
        !output.status.success(),
        "use --all must fail when an attempted switch errors\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("codex"),
        "the failing tool should be named: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `init --json` looked up the rc file for whatever `$SHELL` reported and hit
/// an `unreachable!()` for anything but bash/zsh/fish/pwsh — so it aborted with
/// exit 101 under a plain `/bin/sh`, which is the default in most containers.
#[test]
fn init_json_survives_an_unsupported_shell() {
    for shell in ["/bin/sh", "/bin/dash", "/usr/bin/nu", "/usr/bin/ksh"] {
        let env = TestEnv::new();
        let output = env
            .cmd()
            .env("SHELL", shell)
            .args(["init", "--json", "--no-shell-hook"])
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("panicked"),
            "init --json panicked under SHELL={shell}:\n{stderr}"
        );
        assert!(
            output.status.success(),
            "init --json should succeed under SHELL={shell}: {stderr}"
        );

        let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
        assert_eq!(json["ok"], true);
        assert_eq!(
            json["result"]["shell"]["rc_file"],
            serde_json::Value::Null,
            "an unsupported shell has no aisw rc file"
        );
    }
}

/// AISW_HOME is user-supplied; `--remove-data` must not turn into `rm -rf ~`.
#[test]
fn uninstall_refuses_to_delete_the_user_home_directory() {
    let env = TestEnv::new();
    let output = env
        .cmd()
        .env("AISW_HOME", &env.fake_home)
        .args(["uninstall", "--remove-data", "--yes"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "uninstall must refuse when AISW_HOME is the home directory"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must not be your home directory"),
        "stderr should explain the refusal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(env.fake_home.exists(), "home directory must be untouched");
}
