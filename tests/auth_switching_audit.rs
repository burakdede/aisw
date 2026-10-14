mod common;

use base64::{engine::general_purpose::STANDARD, Engine};
use common::TestEnv;
use predicates::str::contains;
use std::fs;
use std::path::PathBuf;

fn agy_secret(email: &str, token: &str) -> String {
    let payload = serde_json::json!({"email": email, "token": {"access_token": token}});
    format!("go-keyring-base64:{}", STANDARD.encode(payload.to_string()))
}

fn expected_live_secret(secret: &str) -> String {
    if cfg!(target_os = "macos") {
        secret.to_owned()
    } else {
        String::from_utf8(
            STANDARD
                .decode(secret.strip_prefix("go-keyring-base64:").unwrap())
                .unwrap(),
        )
        .unwrap()
    }
}

fn keyring_secret(env: &TestEnv, service: &str, account: &str) -> PathBuf {
    let account = if cfg!(windows) {
        format!(
            "h_{}",
            account
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    } else {
        account.to_owned()
    };
    env.fake_home
        .join("keychain")
        .join(service)
        .join(account)
        .join("secret")
}

fn agy_live_secret(env: &TestEnv) -> PathBuf {
    keyring_secret(env, "gemini", "antigravity")
}

fn seed_agy_live(env: &TestEnv, secret: &str) {
    env.add_fake_tool("agy", "agy 1.3.4");
    let path = agy_live_secret(env);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path.parent().unwrap().join("account"), "antigravity").unwrap();
    fs::write(path, secret).unwrap();
    let app = env.fake_home.join(".gemini/antigravity-cli");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("settings.json"), r#"{"theme":"dark"}"#).unwrap();
}

fn add_agy(env: &TestEnv, name: &str, secret: &str, backend: &str) {
    seed_agy_live(env, secret);
    env.cmd()
        .args([
            "add",
            "antigravity",
            name,
            "--from-live",
            "--credential-backend",
            backend,
        ])
        .assert()
        .success();
}

fn agy_status(env: &TestEnv) -> serde_json::Value {
    let output = env.output(&["status", "--json"]);
    assert!(output.status.success());
    let status: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    status.into_iter().find(|s| s["tool"] == "agy").unwrap()
}

#[test]
fn antigravity_encoded_refresh_survives_switching_for_both_backends() {
    for backend in ["file", "system-keyring"] {
        let env = TestEnv::new();
        let first = agy_secret("first@example.com", "original");
        let second = agy_secret("second@example.com", "second");
        add_agy(&env, "first", &first, backend);
        add_agy(&env, "second", &second, backend);
        env.cmd()
            .args(["use", "antigravity", "first"])
            .assert()
            .success();
        let refreshed = agy_secret("first@example.com", "refreshed");
        fs::write(agy_live_secret(&env), &refreshed).unwrap();
        env.cmd()
            .args(["use", "antigravity", "second"])
            .assert()
            .success();
        env.cmd()
            .args(["use", "antigravity", "first"])
            .assert()
            .success();
        assert_eq!(
            fs::read_to_string(agy_live_secret(&env)).unwrap(),
            expected_live_secret(&refreshed)
        );
        assert_eq!(agy_status(&env)["active_profile_applied"], true);
    }
}

#[test]
fn antigravity_encoded_account_cannot_be_saved_under_another_alias() {
    for backend in ["file", "system-keyring"] {
        let env = TestEnv::new();
        add_agy(
            &env,
            "first",
            &agy_secret("first@example.com", "original"),
            backend,
        );
        fs::write(
            agy_live_secret(&env),
            agy_secret("first@example.com", "refreshed"),
        )
        .unwrap();
        env.cmd()
            .args([
                "add",
                "antigravity",
                "alias",
                "--from-live",
                "--credential-backend",
                backend,
            ])
            .assert()
            .failure()
            .stderr(contains("already exists"));
    }
}

#[test]
fn antigravity_health_ignores_runtime_changes_and_same_account_rotation() {
    let env = TestEnv::new();
    add_agy(
        &env,
        "first",
        &agy_secret("first@example.com", "original"),
        "file",
    );
    let app = env.fake_home.join(".gemini/antigravity-cli");
    fs::create_dir_all(app.join("updater")).unwrap();
    fs::write(app.join("updater/last_check.timestamp"), b"1791936000").unwrap();
    fs::write(app.join("settings.json"), r#"{"theme":"light"}"#).unwrap();
    assert_eq!(agy_status(&env)["active_profile_applied"], true);
    fs::write(
        agy_live_secret(&env),
        agy_secret("first@example.com", "refreshed"),
    )
    .unwrap();
    assert_eq!(agy_status(&env)["active_profile_applied"], true);
    fs::write(
        agy_live_secret(&env),
        agy_secret("other@example.com", "other"),
    )
    .unwrap();
    assert_eq!(agy_status(&env)["active_profile_applied"], false);
    fs::write(app.join("settings.json"), r#"{"modelProvider":"gemini"}"#).unwrap();
    fs::write(
        agy_live_secret(&env),
        agy_secret("first@example.com", "original"),
    )
    .unwrap();
    assert_eq!(agy_status(&env)["active_profile_applied"], false);
}

#[test]
fn antigravity_switch_preserves_global_state_and_ignores_legacy_config_snapshots() {
    let env = TestEnv::new();
    let secret = agy_secret("first@example.com", "original");
    add_agy(&env, "first", &secret, "file");
    let legacy = env.aisw_home.join("profiles/antigravity/first");
    fs::create_dir_all(legacy.join("app/updater")).unwrap();
    fs::create_dir_all(legacy.join("shared")).unwrap();
    fs::write(
        legacy.join("app/settings.json"),
        r#"{"theme":"obsolete","modelProvider":"gemini"}"#,
    )
    .unwrap();
    fs::write(legacy.join("app/updater/last_check.timestamp"), b"old").unwrap();
    fs::write(legacy.join("shared/mcp_config.json"), b"obsolete").unwrap();
    let app = env.fake_home.join(".gemini/antigravity-cli");
    fs::create_dir_all(app.join("updater")).unwrap();
    fs::write(app.join("updater/last_check.timestamp"), b"new").unwrap();
    fs::write(
        app.join("settings.json"),
        r#"{"theme":"light","modelProvider":"gemini","notifications":true}"#,
    )
    .unwrap();
    let global = env.fake_home.join(".gemini/config/mcp_config.json");
    fs::create_dir_all(global.parent().unwrap()).unwrap();
    fs::write(&global, b"current integration").unwrap();
    env.cmd()
        .args(["use", "antigravity", "first"])
        .assert()
        .success();
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(app.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["theme"], "light");
    assert_eq!(settings["notifications"], true);
    assert!(settings.get("modelProvider").is_none());
    assert_eq!(
        fs::read(app.join("updater/last_check.timestamp")).unwrap(),
        b"new"
    );
    assert_eq!(fs::read(global).unwrap(), b"current integration");
}

#[test]
fn antigravity_switch_repairs_hex_wrapped_legacy_credentials() {
    for backend in ["file", "system-keyring"] {
        let env = TestEnv::new();
        let clean = agy_secret("first@example.com", "original");
        add_agy(&env, "first", &clean, backend);
        let legacy: String = format!("{clean}\n")
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect();
        let managed = if backend == "file" {
            env.aisw_home
                .join("profiles/antigravity/first/keyring-secret.json")
        } else {
            keyring_secret(&env, "aisw", "profile:agy:first")
        };
        fs::write(managed, &legacy).unwrap();
        fs::write(agy_live_secret(&env), &legacy).unwrap();
        env.cmd()
            .args(["use", "antigravity", "first"])
            .assert()
            .success();
        assert_eq!(
            fs::read_to_string(agy_live_secret(&env)).unwrap(),
            expected_live_secret(&clean)
        );
        assert_eq!(agy_status(&env)["active_profile_applied"], true);
    }
}

#[test]
fn gemini_switch_and_import_preserve_antigravity_directories() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.64.0");
    let gemini = env.fake_home.join(".gemini");
    fs::create_dir_all(&gemini).unwrap();
    fs::write(
        gemini.join("oauth_creds.json"),
        r#"{"email":"gemini@example.com","token":"original"}"#,
    )
    .unwrap();
    env.cmd()
        .args(["add", "gemini", "work", "--from-live"])
        .assert()
        .success();
    let stored = env.aisw_home.join("profiles/gemini/work");
    for dir in [
        "antigravity-cli",
        "antigravity",
        "antigravity-ide",
        "config",
    ] {
        fs::create_dir_all(gemini.join(dir)).unwrap();
        fs::write(
            gemini.join(dir).join("state.json"),
            b"current Antigravity state",
        )
        .unwrap();
        fs::create_dir_all(stored.join(dir)).unwrap();
        fs::write(
            stored.join(dir).join("state.json"),
            b"old cross-tool capture",
        )
        .unwrap();
    }
    fs::write(
        gemini.join("oauth_creds.json"),
        r#"{"email":"different@example.com","token":"different"}"#,
    )
    .unwrap();
    env.cmd().args(["use", "gemini", "work"]).assert().success();
    for dir in [
        "antigravity-cli",
        "antigravity",
        "antigravity-ide",
        "config",
    ] {
        assert_eq!(
            fs::read(gemini.join(dir).join("state.json")).unwrap(),
            b"current Antigravity state"
        );
    }
    let output = env.output(&["status", "--json"]);
    let status: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        status.iter().find(|s| s["tool"] == "gemini").unwrap()["active_profile_applied"],
        true
    );
    fs::write(
        gemini.join("oauth_creds.json"),
        r#"{"email":"other@example.com","token":"other"}"#,
    )
    .unwrap();
    env.cmd()
        .args(["add", "gemini", "other", "--from-live"])
        .assert()
        .success();
    for dir in [
        "antigravity-cli",
        "antigravity",
        "antigravity-ide",
        "config",
    ] {
        assert!(!env
            .aisw_home
            .join("profiles/gemini/other")
            .join(dir)
            .exists());
    }
}

#[test]
fn antigravity_rejects_oauth_capture_while_the_api_key_provider_is_active() {
    let env = TestEnv::new();
    seed_agy_live(&env, &agy_secret("first@example.com", "cached"));
    fs::write(
        env.fake_home.join(".gemini/antigravity-cli/settings.json"),
        r#"{"modelProvider":"gemini"}"#,
    )
    .unwrap();
    env.cmd()
        .args(["add", "antigravity", "wrong", "--from-live"])
        .assert()
        .failure()
        .stderr(contains("API-key provider"));
    assert!(!env.aisw_home.join("profiles/antigravity/wrong").exists());
}

#[test]
fn antigravity_switch_rejects_malformed_envelopes_without_changing_live_state() {
    let env = TestEnv::new();
    let secret = agy_secret("first@example.com", "original");
    add_agy(&env, "first", &secret, "file");
    fs::write(
        env.aisw_home
            .join("profiles/antigravity/first/keyring-secret.json"),
        b"go-keyring-base64:not-base64",
    )
    .unwrap();
    // A different live account must not synchronize over the broken managed capture.
    let other = agy_secret("other@example.com", "other");
    fs::write(agy_live_secret(&env), &other).unwrap();
    env.cmd()
        .args(["use", "antigravity", "first"])
        .assert()
        .failure();
    assert_eq!(fs::read_to_string(agy_live_secret(&env)).unwrap(), other);
}

#[test]
fn antigravity_provider_round_trip_preserves_preferences() {
    let env = TestEnv::new();
    let secret = agy_secret("first@example.com", "original");
    add_agy(&env, "first", &secret, "file");
    env.cmd()
        .args([
            "add",
            "antigravity",
            "api",
            "--api-key",
            "AIza-api-key-test",
        ])
        .assert()
        .success();
    env.cmd()
        .args(["use", "antigravity", "api"])
        .assert()
        .success();
    let settings_path = env.fake_home.join(".gemini/antigravity-cli/settings.json");
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["modelProvider"], "gemini");
    assert_eq!(settings["theme"], "dark");
    env.cmd()
        .args(["use", "antigravity", "first"])
        .assert()
        .success();
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    assert!(settings.get("modelProvider").is_none());
    assert_eq!(settings["theme"], "dark");
    assert_eq!(
        fs::read_to_string(agy_live_secret(&env)).unwrap(),
        expected_live_secret(&secret)
    );
}

#[test]
fn detection_does_not_claim_a_cached_antigravity_oauth_session_for_api_key_mode() {
    let env = TestEnv::new();
    seed_agy_live(&env, &agy_secret("cached@example.com", "cached"));
    fs::write(
        env.fake_home.join(".gemini/antigravity-cli/settings.json"),
        r#"{"modelProvider":"gemini"}"#,
    )
    .unwrap();
    let output = env.output(&["init", "--json", "--no-shell-hook", "--detect-live"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let agy = report["result"]["live_accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["tool"] == "agy")
        .unwrap();
    assert_eq!(agy["outcome"], "local_state_without_importable_auth");
    assert!(agy["auth_method"].is_null());
}

#[test]
fn gemini_encrypted_storage_rejects_file_switch_without_mutating_credentials() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.64.0");
    let credentials = env.fake_home.join(".gemini/oauth_creds.json");
    fs::create_dir_all(credentials.parent().unwrap()).unwrap();
    fs::write(
        &credentials,
        r#"{"email":"work@example.com","token":"stored"}"#,
    )
    .unwrap();
    env.cmd()
        .args(["add", "gemini", "work", "--from-live"])
        .assert()
        .success();
    let live = br#"{"email":"other@example.com","token":"other"}"#;
    fs::write(&credentials, live).unwrap();
    env.cmd()
        .env("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE", "true")
        .args(["use", "gemini", "work"])
        .assert()
        .failure()
        .stderr(contains("encrypted"));
    assert_eq!(fs::read(credentials).unwrap(), live);
}

#[test]
fn gemini_health_checks_authentication_instead_of_mutable_runtime_files() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.64.0");
    let root = env.fake_home.join(".gemini");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("oauth_creds.json"),
        r#"{"email":"work@example.com","token":"original"}"#,
    )
    .unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#,
    )
    .unwrap();
    env.cmd()
        .args(["add", "gemini", "work", "--from-live"])
        .assert()
        .success();
    fs::create_dir_all(root.join("tmp/project")).unwrap();
    fs::write(root.join("tmp/project/chat.json"), b"runtime state").unwrap();
    let status = || {
        let output = env.output(&["status", "--json"]);
        let values: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        values.into_iter().find(|s| s["tool"] == "gemini").unwrap()["active_profile_applied"]
            .clone()
    };
    assert_eq!(status(), true);
    fs::write(root.join(".env"), b"NO_COLOR=1\n").unwrap();
    assert_eq!(status(), true);
    fs::write(
        root.join("oauth_creds.json"),
        r#"{"email":"work@example.com","token":"refreshed"}"#,
    )
    .unwrap();
    assert_eq!(status(), true);
    fs::write(
        root.join("oauth_creds.json"),
        r#"{"email":"other@example.com","token":"other"}"#,
    )
    .unwrap();
    assert_eq!(status(), false);
    fs::write(
        root.join("oauth_creds.json"),
        r#"{"email":"work@example.com","token":"original"}"#,
    )
    .unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{"security":{"auth":{"selectedType":"gemini-api-key"}}}"#,
    )
    .unwrap();
    assert_eq!(status(), false);
}

#[test]
fn gemini_provider_round_trip_selects_the_requested_authentication() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.64.0");
    let root = env.fake_home.join(".gemini");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("oauth_creds.json"),
        r#"{"email":"work@example.com","token":"stored"}"#,
    )
    .unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{"theme":"dark","security":{"auth":{"selectedType":"oauth-personal"}}}"#,
    )
    .unwrap();
    env.cmd()
        .args(["add", "gemini", "oauth", "--from-live"])
        .assert()
        .success();
    env.cmd()
        .args(["add", "gemini", "api", "--api-key", "AIza-test-api-key"])
        .assert()
        .success();
    env.cmd().args(["use", "gemini", "api"]).assert().success();
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(
        settings.pointer("/security/auth/selectedType").unwrap(),
        "gemini-api-key"
    );
    assert_eq!(settings["theme"], "dark");
    env.cmd()
        .args(["use", "gemini", "oauth"])
        .assert()
        .success();
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(
        settings.pointer("/security/auth/selectedType").unwrap(),
        "oauth-personal"
    );
    assert!(!root.join(".env").exists());
}

#[test]
fn gemini_import_preserves_vertex_key_provider_selection() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.64.0");
    let root = env.fake_home.join(".gemini");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".env"), b"GEMINI_API_KEY=AIza-vertex-test-key\n").unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{"security":{"auth":{"selectedType":"vertex-ai"}}}"#,
    )
    .unwrap();
    env.cmd()
        .args(["add", "gemini", "vertex", "--from-live"])
        .assert()
        .success();
    fs::write(
        root.join("settings.json"),
        r#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#,
    )
    .unwrap();
    env.cmd()
        .args(["use", "gemini", "vertex"])
        .assert()
        .success();
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(
        settings.pointer("/security/auth/selectedType").unwrap(),
        "vertex-ai"
    );
    let output = env.output(&["status", "--json"]);
    let values: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        values.into_iter().find(|s| s["tool"] == "gemini").unwrap()["active_profile_applied"],
        true
    );
}

#[test]
fn gemini_live_import_uses_the_selected_oauth_provider_over_a_dormant_key() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.64.0");
    let root = env.fake_home.join(".gemini");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".env"), b"GEMINI_API_KEY=AIza-dormant-key\n").unwrap();
    fs::write(
        root.join("oauth_creds.json"),
        r#"{"email":"work@example.com","token":"live"}"#,
    )
    .unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#,
    )
    .unwrap();
    env.cmd()
        .args(["add", "gemini", "oauth", "--from-live"])
        .assert()
        .success();
    let config: serde_json::Value =
        serde_json::from_str(&env.read_home_file("config.json")).unwrap();
    assert_eq!(
        config["profiles"]["gemini"]["oauth"]["auth_method"],
        "o_auth"
    );
    assert!(!root.join(".env").exists());
}

#[test]
#[cfg(unix)]
fn gemini_env_capture_preserves_the_source_symlink_guard() {
    let env = TestEnv::new();
    env.add_fake_tool("gemini", "gemini 0.64.0");
    let root = env.fake_home.join(".gemini");
    fs::create_dir_all(&root).unwrap();
    let source = env.dir.path().join("outside.env");
    fs::write(&source, b"GEMINI_API_KEY=AIza-synthetic-key\n").unwrap();
    std::os::unix::fs::symlink(&source, root.join(".env")).unwrap();
    env.cmd()
        .args(["add", "gemini", "unsafe", "--from-live"])
        .assert()
        .failure()
        .stderr(contains("symlink"));
    assert!(!env.aisw_home.join("profiles/gemini/unsafe").exists());
    assert_eq!(
        fs::read(source).unwrap(),
        b"GEMINI_API_KEY=AIza-synthetic-key\n"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn antigravity_restores_mac_envelopes_as_plain_go_credentials_without_identity() {
    let env = TestEnv::new();
    let payload = serde_json::json!({"access_token":"opaque-token", "name":"Gödel 👋"});
    let envelope = format!("go-keyring-base64:{}", STANDARD.encode(payload.to_string()));
    add_agy(&env, "portable", &envelope, "file");
    env.cmd()
        .args(["use", "antigravity", "portable"])
        .assert()
        .success();
    let live: serde_json::Value =
        serde_json::from_slice(&fs::read(agy_live_secret(&env)).unwrap()).unwrap();
    assert_eq!(live, payload);
    assert_eq!(agy_status(&env)["active_profile_applied"], true);
}
