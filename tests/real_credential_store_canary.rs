mod common;

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use common::TestEnv;

const KEYRING_SERVICE: &str = "aisw";

#[cfg(target_os = "linux")]
struct SecretServiceFixtureItem<'a>(dbus_secret_service::Item<'a>);

#[cfg(target_os = "linux")]
impl Drop for SecretServiceFixtureItem<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.0.delete() {
            eprintln!("could not delete Secret Service fixture item: {error}");
        }
    }
}

struct CanaryCleanup {
    entries: Vec<CanaryEntry>,
}

struct CanaryEntry {
    service: String,
    account: String,
    entry: keyring::Entry,
    previous: Option<Vec<u8>>,
}

impl CanaryCleanup {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn track(&mut self, account: String) -> Result<(), keyring::Error> {
        self.track_entry(KEYRING_SERVICE, account).map(|_| ())
    }

    fn track_entry(
        &mut self,
        service: &str,
        account: impl Into<String>,
    ) -> Result<Option<Vec<u8>>, keyring::Error> {
        let account = account.into();
        let entry = keyring::Entry::new(service, &account)?;
        self.track_native_entry(service, &account, entry)
    }

    fn track_native_entry(
        &mut self,
        service: &str,
        account: &str,
        entry: keyring::Entry,
    ) -> Result<Option<Vec<u8>>, keyring::Error> {
        eprintln!("canary: inspect native entry {service}/{account}");
        let previous = match entry.get_secret() {
            Ok(secret) => Some(secret),
            Err(keyring::Error::NoEntry) => None,
            Err(error) => return Err(error),
        };
        eprintln!("canary: inspected native entry {service}/{account}");
        self.entries.push(CanaryEntry {
            service: service.to_owned(),
            account: account.to_owned(),
            entry,
            previous: previous.clone(),
        });
        Ok(previous)
    }

    fn restore(&self) -> Result<(), String> {
        let mut failures = Vec::new();

        for tracked in &self.entries {
            let result = if let Some(previous) = &tracked.previous {
                tracked.entry.set_secret(previous)
            } else {
                match tracked.entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                    Err(error) => Err(error),
                }
            };

            if let Err(error) = result {
                failures.push(format!("{}/{}: {error}", tracked.service, tracked.account));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

impl Drop for CanaryCleanup {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("credential-store canary cleanup failed during unwind: {error}");
        }
    }
}

fn assert_entry_absent(service: &str, account: &str) {
    let entry = keyring::Entry::new(service, account).unwrap();
    match entry.get_password() {
        Err(keyring::Error::NoEntry) => {}
        Ok(_) => panic!("canary entry was not removed: {service}/{account}"),
        Err(error) => panic!("could not verify canary cleanup for {service}/{account}: {error}"),
    }
}

#[cfg(target_os = "macos")]
fn preauthorize_ci_keychain_entry(service: &str, account: &str) {
    if std::env::var_os("GITHUB_ACTIONS").is_none() {
        return;
    }
    let output = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-A",
            "-s",
            service,
            "-a",
            account,
            "-w",
            "aisw-canary-placeholder",
        ])
        .output()
        .expect("security should be available on macOS");
    assert!(
        output.status.success(),
        "could not preauthorize disposable keychain entry {service}/{account}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn canary_suffix() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after UNIX_EPOCH")
        .as_millis();
    format!("{}-{}", std::process::id(), millis)
}

#[cfg(unix)]
fn write_fake_tool(bin_dir: &Path, name: &str, version: &str) {
    let path = bin_dir.join(name);
    fs::write(&path, format!("#!/bin/sh\necho '{}'\nexit 0\n", version)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(windows)]
fn write_fake_tool(bin_dir: &Path, name: &str, version: &str) {
    let path = bin_dir.join(format!("{name}.cmd"));
    fs::write(
        &path,
        format!("@echo off\r\necho {}\r\nexit /b 0\r\n", version),
    )
    .unwrap();
}

fn canary_cmd(env: &TestEnv, bin_dir: &Path, args: &[&str]) -> std::process::Output {
    eprintln!("canary: run aisw {}", args.join(" "));
    let mut cmd = Command::cargo_bin("aisw").expect("aisw binary not found");
    cmd.args(args)
        .env("AISW_HOME", &env.aisw_home)
        .env("HOME", &env.fake_home)
        .env("AISW_TEST_USER_HOME", &env.fake_home)
        .env("PATH", bin_dir)
        .env_remove("AISW_KEYRING_TEST_DIR")
        .env_remove("AISW_SECURITY_BIN")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME");
    #[cfg(target_os = "macos")]
    {
        cmd.env(
            "HOME",
            std::env::var("HOME").expect("macOS runner home should be set"),
        );
        cmd.env(
            "PATH",
            std::env::join_paths([bin_dir, Path::new("/usr/bin"), Path::new("/bin")]).unwrap(),
        );
    }
    #[cfg(windows)]
    {
        // `dirs::home_dir()` uses the Windows profile known folder, not HOME.
        // Keep the canary's live-state writes inside its temporary sandbox.
        let roaming = env.fake_home.join("AppData").join("Roaming");
        let local = env.fake_home.join("AppData").join("Local");
        fs::create_dir_all(&roaming).expect("failed to create fake AppData/Roaming");
        fs::create_dir_all(&local).expect("failed to create fake AppData/Local");
        cmd.env_remove("HOMEDRIVE")
            .env_remove("HOMEPATH")
            .env("USERPROFILE", &env.fake_home)
            .env("APPDATA", roaming)
            .env("LOCALAPPDATA", local);
    }
    cmd.output().unwrap()
}

fn assert_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{} failed\nstdout:\n{}\nstderr:\n{}",
        context,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn json_output(env: &TestEnv, bin_dir: &Path, args: &[&str]) -> serde_json::Value {
    let output = canary_cmd(env, bin_dir, args);
    assert_success(&output, &format!("aisw {}", args.join(" ")));
    serde_json::from_slice(&output.stdout).expect("stdout should be valid json")
}

fn write_config_for_profiles(
    env: &TestEnv,
    claude_profiles: &[&str],
    codex_profiles: &[&str],
    antigravity_profiles: &[&str],
) {
    let claude_json: serde_json::Map<String, serde_json::Value> = claude_profiles
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                serde_json::json!({
                    "added_at": "2026-01-01T00:00:00Z",
                    "auth_method": if name.contains("oauth") { "o_auth" } else { "api_key" },
                    "credential_backend": "system_keyring",
                    "label": null
                }),
            )
        })
        .collect();
    let codex_json: serde_json::Map<String, serde_json::Value> = codex_profiles
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                serde_json::json!({
                    "added_at": "2026-01-01T00:00:00Z",
                    "auth_method": if name.contains("oauth") { "o_auth" } else { "api_key" },
                    "credential_backend": "system_keyring",
                    "label": null
                }),
            )
        })
        .collect();
    let antigravity_json: serde_json::Map<String, serde_json::Value> = antigravity_profiles
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                serde_json::json!({
                    "added_at": "2026-01-01T00:00:00Z",
                    "auth_method": "o_auth",
                    "credential_backend": "system_keyring",
                    "label": null
                }),
            )
        })
        .collect();

    let config = serde_json::json!({
        "version": 1,
        "active": {"claude": null, "codex": null, "gemini": null, "antigravity": null},
        "profiles": {
            "claude": claude_json,
            "codex": codex_json,
            "gemini": {},
            "antigravity": antigravity_json
        },
        "settings": {"backup_on_switch": true, "max_backups": 10}
    });
    fs::write(
        env.aisw_home.join("config.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "opt-in real credential-store canary; set AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY=1"]
fn real_credential_store_canary_covers_secure_auth_modes() {
    if std::env::var("AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY").as_deref() != Ok("1") {
        eprintln!(
            "skipping real credential-store canary: set AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY=1"
        );
        return;
    }

    #[cfg(target_os = "macos")]
    let _interaction_lock = std::env::var_os("GITHUB_ACTIONS").map(|_| {
        security_framework::os::macos::keychain::SecKeychain::disable_user_interaction()
            .expect("hosted canary must fail instead of waiting for Keychain UI")
    });

    let env = TestEnv::new();
    let bin_dir = env.dir.path().join("canary-bin");
    fs::create_dir_all(&bin_dir).unwrap();

    write_fake_tool(&bin_dir, "claude", "2.1.87 (Claude Code)");
    write_fake_tool(&bin_dir, "codex", "codex-cli 0.117.0");
    write_fake_tool(&bin_dir, "agy", "agy 1.1.28");

    let suffix = canary_suffix();
    let claude_oauth = format!("claude-oauth-{suffix}");
    let claude_api = format!("claude-api-{suffix}");
    let codex_oauth = format!("codex-oauth-{suffix}");
    let codex_api = format!("codex-api-{suffix}");
    let antigravity = format!("antigravity-{suffix}");

    fs::create_dir_all(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join(&claude_oauth),
    )
    .unwrap();
    fs::create_dir_all(
        env.aisw_home
            .join("profiles")
            .join("claude")
            .join(&claude_api),
    )
    .unwrap();

    let codex_oauth_dir = env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join(&codex_oauth);
    let codex_api_dir = env
        .aisw_home
        .join("profiles")
        .join("codex")
        .join(&codex_api);
    fs::create_dir_all(&codex_oauth_dir).unwrap();
    fs::create_dir_all(&codex_api_dir).unwrap();
    fs::write(
        codex_oauth_dir.join("config.toml"),
        b"cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();

    let antigravity_dir = env
        .aisw_home
        .join("profiles")
        .join("antigravity")
        .join(&antigravity);
    fs::create_dir_all(&antigravity_dir).unwrap();
    fs::write(
        antigravity_dir.join("keyring.json"),
        br#"{"service":"gemini","account":"antigravity"}"#,
    )
    .unwrap();
    fs::create_dir_all(antigravity_dir.join("app")).unwrap();
    fs::write(
        antigravity_dir.join("app").join("settings.json"),
        br#"{"modelProvider":"gemini"}"#,
    )
    .unwrap();
    fs::create_dir_all(antigravity_dir.join("shared").join("projects")).unwrap();
    fs::write(
        antigravity_dir
            .join("shared")
            .join("projects")
            .join("repo.json"),
        br#"{"mode":"plan"}"#,
    )
    .unwrap();
    fs::write(
        codex_api_dir.join("config.toml"),
        b"cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();

    write_config_for_profiles(
        &env,
        &[&claude_oauth, &claude_api],
        &[&codex_oauth, &codex_api],
        &[&antigravity],
    );

    let mut cleanup = CanaryCleanup::new();
    let claude_oauth_account = format!("profile:claude:{claude_oauth}");
    let claude_api_account = format!("profile:claude:{claude_api}");
    let codex_oauth_account = format!("profile:codex:{codex_oauth}");
    let codex_api_account = format!("profile:codex:{codex_api}");
    let antigravity_account = format!("profile:agy:{antigravity}");
    cleanup.track(claude_oauth_account.clone()).unwrap();
    cleanup.track(claude_api_account.clone()).unwrap();
    cleanup.track(codex_oauth_account.clone()).unwrap();
    cleanup.track(codex_api_account.clone()).unwrap();
    cleanup.track(antigravity_account.clone()).unwrap();
    #[cfg(windows)]
    let go_live_entry =
        keyring::Entry::new_with_target("gemini:antigravity", "gemini", "antigravity").unwrap();
    #[cfg(target_os = "linux")]
    let go_live_entry = keyring::Entry::new_with_credential(Box::new(
        keyring::secret_service::SsCredential::new_with_no_target("gemini", "antigravity").unwrap(),
    ));
    #[cfg(not(any(windows, target_os = "linux")))]
    let go_live_entry = keyring::Entry::new("gemini", "antigravity").unwrap();
    let previous_antigravity_live_secret = cleanup
        .track_native_entry("gemini", "antigravity", go_live_entry)
        .unwrap();

    #[cfg(windows)]
    {
        let shadow = keyring::Entry::new("gemini", "antigravity").unwrap();
        cleanup
            .track_native_entry("Rust shadow", "antigravity", shadow)
            .unwrap();
    }

    #[cfg(target_os = "macos")]
    {
        for account in [
            &claude_oauth_account,
            &claude_api_account,
            &codex_oauth_account,
            &codex_api_account,
            &antigravity_account,
        ] {
            preauthorize_ci_keychain_entry(KEYRING_SERVICE, account);
        }
        preauthorize_ci_keychain_entry("gemini", "antigravity");
    }

    eprintln!("canary: seed native credentials");
    keyring::Entry::new(KEYRING_SERVICE, &claude_oauth_account)
        .unwrap()
        .set_password(r#"{"claudeAiOauth":{"accessToken":"real-claude-oauth-token"}}"#)
        .unwrap();
    keyring::Entry::new(KEYRING_SERVICE, &claude_api_account)
        .unwrap()
        .set_password(r#"{"apiKey":"sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#)
        .unwrap();
    keyring::Entry::new(KEYRING_SERVICE, &codex_oauth_account)
        .unwrap()
        .set_password(r#"{"account":{"email":"real@example.com"},"token":"codex-oauth-token"}"#)
        .unwrap();
    keyring::Entry::new(KEYRING_SERVICE, &codex_api_account)
        .unwrap()
        .set_password(r#"{"token":"sk-codex-real-canary-token"}"#)
        .unwrap();
    keyring::Entry::new(KEYRING_SERVICE, &antigravity_account)
        .unwrap()
        .set_password(r#"{"email":"real-antigravity@example.com"}"#)
        .unwrap();

    eprintln!("canary: native credentials seeded");
    #[cfg(target_os = "linux")]
    {
        use dbus_secret_service::{EncryptionType, SecretService};
        use std::collections::HashMap;
        let ss = SecretService::connect(EncryptionType::Plain).unwrap();
        let collection = ss
            .get_all_collections()
            .unwrap()
            .into_iter()
            .find(|collection| collection.path == "/org/freedesktop/secrets/collection/login")
            .unwrap_or_else(|| ss.get_default_collection().unwrap());
        collection.ensure_unlocked().unwrap();
        let item = collection
            .create_item(
                "Go native fixture",
                HashMap::from([("service", "gemini"), ("username", "antigravity")]),
                br#"{"email":"go-live@example.com","access_token":"first"}"#,
                true,
                "text/plain; charset=utf-8",
            )
            .unwrap();
        let session = ss.get_collection_by_alias("session").unwrap();
        assert_ne!(session.path, collection.path);
        session.ensure_unlocked().unwrap();
        let shadow = SecretServiceFixtureItem(
            session
                .create_item(
                    "Rust collection shadow",
                    HashMap::from([
                        ("service", "gemini"),
                        ("username", "antigravity"),
                        ("target", "default"),
                    ]),
                    br#"{"email":"session-shadow@example.com"}"#,
                    true,
                    "text/plain; charset=utf-8",
                )
                .unwrap(),
        );
        let imported = format!("go-import-{suffix}");
        assert_success(
            &canary_cmd(
                &env,
                &bin_dir,
                &[
                    "add",
                    "antigravity",
                    &imported,
                    "--from-live",
                    "--credential-backend",
                    "file",
                ],
            ),
            "capture Go Linux credential",
        );
        let captured: serde_json::Value = serde_json::from_slice(
            &fs::read(
                env.aisw_home
                    .join("profiles/antigravity")
                    .join(&imported)
                    .join("keyring-secret.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(captured["email"], "go-live@example.com");
        drop(shadow);
        assert_success(
            &canary_cmd(&env, &bin_dir, &["use", "antigravity", &imported]),
            "activate Go Linux fixture",
        );
        item.set_secret(
            br#"{"email":"go-live@example.com","access_token":"rotated"}"#,
            "text/plain; charset=utf-8",
        )
        .unwrap();
        assert_success(
            &canary_cmd(&env, &bin_dir, &["use", "antigravity", &antigravity]),
            "switch after direct Go token rotation",
        );
        let captured: serde_json::Value = serde_json::from_slice(
            &fs::read(
                env.aisw_home
                    .join("profiles/antigravity")
                    .join(&imported)
                    .join("keyring-secret.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(captured["access_token"], "rotated");
        assert_success(
            &canary_cmd(
                &env,
                &bin_dir,
                &["remove", "antigravity", &imported, "--yes"],
            ),
            "remove Go Linux fixture",
        );
    }
    #[cfg(windows)]
    {
        let shadow = keyring::Entry::new("gemini", "antigravity").unwrap();
        shadow
            .set_password(r#"{"email":"rust-shadow@example.com"}"#)
            .unwrap();
        let go_entry =
            keyring::Entry::new_with_target("gemini:antigravity", "gemini", "antigravity").unwrap();
        go_entry
            .set_secret("{\"email\":\"go-live@example.com\",\"name\":\"Gödel 👋\"}".as_bytes())
            .unwrap();
        let imported = format!("go-import-{suffix}");
        assert_success(
            &canary_cmd(
                &env,
                &bin_dir,
                &[
                    "add",
                    "antigravity",
                    &imported,
                    "--from-live",
                    "--credential-backend",
                    "file",
                ],
            ),
            "capture Go Windows credential",
        );
        let captured: serde_json::Value = serde_json::from_slice(
            &fs::read(
                env.aisw_home
                    .join("profiles/antigravity")
                    .join(&imported)
                    .join("keyring-secret.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(captured["email"], "go-live@example.com");
        assert_eq!(captured["name"], "Gödel 👋");
        assert_success(
            &canary_cmd(&env, &bin_dir, &["use", "antigravity", &antigravity]),
            "switch away from captured Go fixture",
        );
        assert_success(
            &canary_cmd(
                &env,
                &bin_dir,
                &["remove", "antigravity", &imported, "--yes"],
            ),
            "remove imported Go fixture",
        );
    }
    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "claude", &claude_oauth]),
        "claude oauth use",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "claude", &claude_api]),
        "claude api use",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "codex", &codex_oauth]),
        "codex oauth use",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "codex", &codex_api]),
        "codex api use",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["use", "antigravity", &antigravity]),
        "antigravity use",
    );
    #[cfg(windows)]
    {
        let go_entry =
            keyring::Entry::new_with_target("gemini:antigravity", "gemini", "antigravity").unwrap();
        assert_eq!(
            go_entry.get_secret().unwrap(),
            br#"{"email":"real-antigravity@example.com"}"#
        );
        assert_eq!(
            keyring::Entry::new("gemini", "antigravity")
                .unwrap()
                .get_password()
                .unwrap(),
            r#"{"email":"rust-shadow@example.com"}"#
        );
    }

    assert!(
        env.fake_home.join(".codex").join("auth.json").is_file(),
        "Codex live credentials should stay inside the canary home"
    );

    let status = json_output(&env, &bin_dir, &["status", "--json"]);
    let status_rows = status.as_array().unwrap();

    let claude_row = status_rows
        .iter()
        .find(|row| row["tool"] == "claude")
        .expect("claude status row should exist");
    assert_eq!(claude_row["active_profile"], claude_api);
    assert_eq!(claude_row["credential_backend"], "system_keyring");
    assert_eq!(claude_row["credentials_present"], true);

    let codex_row = status_rows
        .iter()
        .find(|row| row["tool"] == "codex")
        .expect("codex status row should exist");
    assert_eq!(codex_row["active_profile"], codex_api);
    assert_eq!(codex_row["credential_backend"], "system_keyring");
    assert_eq!(codex_row["credentials_present"], true);

    let antigravity_row = status_rows
        .iter()
        .find(|row| row["tool"] == "agy")
        .expect("antigravity status row should exist");
    assert_eq!(antigravity_row["active_profile"], antigravity);
    assert_eq!(antigravity_row["credential_backend"], "system_keyring");
    assert_eq!(antigravity_row["credentials_present"], true);
    assert!(!env
        .fake_home
        .join(".gemini/antigravity-cli/settings.json")
        .exists());
    assert!(!env
        .fake_home
        .join(".gemini/config/projects/repo.json")
        .exists());

    let list = json_output(&env, &bin_dir, &["list", "--json"]);
    assert_eq!(list["claude"]["active"], claude_api);
    assert_eq!(list["codex"]["active"], codex_api);
    assert_eq!(list["agy"]["active"], antigravity);

    assert_success(
        &canary_cmd(
            &env,
            &bin_dir,
            &["remove", "claude", &claude_oauth, "--yes"],
        ),
        "claude oauth remove",
    );
    assert_success(
        &canary_cmd(
            &env,
            &bin_dir,
            &["remove", "claude", &claude_api, "--yes", "--force"],
        ),
        "claude api remove",
    );
    assert_success(
        &canary_cmd(&env, &bin_dir, &["remove", "codex", &codex_oauth, "--yes"]),
        "codex oauth remove",
    );
    assert_success(
        &canary_cmd(
            &env,
            &bin_dir,
            &["remove", "codex", &codex_api, "--yes", "--force"],
        ),
        "codex api remove",
    );
    assert_success(
        &canary_cmd(
            &env,
            &bin_dir,
            &["remove", "antigravity", &antigravity, "--yes", "--force"],
        ),
        "antigravity remove",
    );

    for tracked in &cleanup.entries[..5] {
        assert_entry_absent(&tracked.service, &tracked.account);
    }

    cleanup
        .restore()
        .unwrap_or_else(|error| panic!("credential-store canary cleanup failed: {error}"));
    let restored_live_secret = match cleanup.entries[5].entry.get_secret() {
        Ok(secret) => Some(secret),
        Err(keyring::Error::NoEntry) => None,
        Err(error) => panic!("could not verify restored canary entry: {error}"),
    };
    assert_eq!(restored_live_secret, previous_antigravity_live_secret);
}
