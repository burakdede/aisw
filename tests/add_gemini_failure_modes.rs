//! Failure-mode tests for the Gemini OAuth HOME-override capture flow.
//!
//! These tests use mock binaries and fixture files — no real Gemini binary,
//! no network. Each test injects a short timeout rather than relying on
//! the production 120s timeout.

use std::sync::Mutex;
use std::time::Duration;

use tempfile::tempdir;

use aisw::auth::gemini;
use aisw::config::ConfigStore;
use aisw::profile::ProfileStore;

static SPAWN_LOCK: Mutex<()> = Mutex::new(());

const TEST_POLL: Duration = Duration::from_millis(10);

fn make_mock_binary(dir: &std::path::Path, name: &str, script_body: &str) -> std::path::PathBuf {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join(name);
    fs::write(&bin, format!("#!/bin/sh\n{}", script_body)).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn make_fixture_jwt(email: &str) -> String {
    let payload_json = format!(r#"{{"email":"{}","exp":9999999999}}"#, email);
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut b64 = String::new();
    for chunk in payload_json.as_bytes().chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        b64.push(alphabet[((triple >> 18) & 0x3F) as usize] as char);
        b64.push(alphabet[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            b64.push(alphabet[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            b64.push('=');
        }
        if chunk.len() > 2 {
            b64.push(alphabet[(triple & 0x3F) as usize] as char);
        } else {
            b64.push('=');
        }
    }
    format!("eyJhbGciOiJIUzI1NiJ9.{}.sig", b64)
}

/// Mock binary that does nothing — no credentials written, sleeps briefly.
/// With a short timeout, this should return a "timed out" error.
#[test]
#[cfg(unix)]
fn empty_capture_dir_after_timeout() {
    let _g = SPAWN_LOCK
        .lock()
        .unwrap_or_else(|p: std::sync::PoisonError<_>| p.into_inner());
    let tmp = tempdir().unwrap();
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    // Binary sleeps 2s — long enough to outlast the 200ms timeout
    let bin = make_mock_binary(&bin_dir, "gemini", "sleep 2\n");

    let ps = ProfileStore::new(tmp.path());
    let cs = ConfigStore::new(tmp.path());
    let err = gemini::add_oauth_with_for_test(
        &ps,
        &cs,
        "work",
        None,
        &bin,
        Duration::from_millis(200),
        TEST_POLL,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("timed out"),
        "unexpected error: {}",
        err
    );
}

/// Mock binary writes malformed JSON to oauth_creds.json.
/// Capture should succeed (file is present), but identity extraction returns None.
#[test]
#[cfg(unix)]
fn malformed_oauth_creds_json_does_not_panic() {
    let _g = SPAWN_LOCK
        .lock()
        .unwrap_or_else(|p: std::sync::PoisonError<_>| p.into_inner());
    let tmp = tempdir().unwrap();
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin = make_mock_binary(
        &bin_dir,
        "gemini",
        "mkdir -p \"$GEMINI_CLI_HOME/.gemini\"\nprintf '{NOT VALID JSON' > \"$GEMINI_CLI_HOME/.gemini/oauth_creds.json\"\nexit 0\n",
    );

    let ps = ProfileStore::new(tmp.path());
    let cs = ConfigStore::new(tmp.path());
    // This should succeed (file is present, identity will be None but no panic)
    let result = gemini::add_oauth_with_for_test(
        &ps,
        &cs,
        "work",
        None,
        &bin,
        Duration::from_secs(2),
        TEST_POLL,
    );

    // Profile should be created even if identity can't be extracted
    assert!(result.is_ok(), "unexpected error: {:?}", result.err());
    assert!(ps.exists(aisw::types::Tool::Gemini, "work"));

    // Identity extraction should return None gracefully (no panic)
    let identity = gemini::extract_captured_identity(&ps, "work");
    assert!(identity.is_none());
}

/// Mock binary writes valid JSON but without id_token.
/// Capture succeeds, identity returns None gracefully.
#[test]
#[cfg(unix)]
fn oauth_creds_missing_id_token() {
    let _g = SPAWN_LOCK
        .lock()
        .unwrap_or_else(|p: std::sync::PoisonError<_>| p.into_inner());
    let tmp = tempdir().unwrap();
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin = make_mock_binary(
        &bin_dir,
        "gemini",
        "mkdir -p \"$GEMINI_CLI_HOME/.gemini\"\nprintf '{\"email\":\"none\"}' > \"$GEMINI_CLI_HOME/.gemini/oauth_creds.json\"\nexit 0\n",
    );

    let ps = ProfileStore::new(tmp.path());
    let cs = ConfigStore::new(tmp.path());
    let result = gemini::add_oauth_with_for_test(
        &ps,
        &cs,
        "work",
        None,
        &bin,
        Duration::from_secs(2),
        TEST_POLL,
    );

    assert!(result.is_ok(), "unexpected error: {:?}", result.err());
    let identity = gemini::extract_captured_identity(&ps, "work");
    assert!(
        identity.is_none(),
        "expected None identity, got {:?}",
        identity
    );
}

/// Mock binary writes a valid JWT in id_token — identity should be extracted.
#[test]
#[cfg(unix)]
fn valid_fixture_jwt_identity_extracted() {
    let _g = SPAWN_LOCK
        .lock()
        .unwrap_or_else(|p: std::sync::PoisonError<_>| p.into_inner());
    let tmp = tempdir().unwrap();
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let jwt = make_fixture_jwt("test@example.com");
    // Write a script that writes the oauth_creds.json with the JWT
    let script = format!(
        "mkdir -p \"$GEMINI_CLI_HOME/.gemini\"\nprintf '{{\"id_token\":\"{}\"}}' > \"$GEMINI_CLI_HOME/.gemini/oauth_creds.json\"\nexit 0\n",
        jwt
    );
    let bin = make_mock_binary(&bin_dir, "gemini", &script);

    let ps = ProfileStore::new(tmp.path());
    let cs = ConfigStore::new(tmp.path());
    gemini::add_oauth_with_for_test(
        &ps,
        &cs,
        "work",
        None,
        &bin,
        Duration::from_secs(2),
        TEST_POLL,
    )
    .unwrap();

    let identity = gemini::extract_captured_identity(&ps, "work");
    assert_eq!(
        identity.as_deref(),
        Some("test@example.com"),
        "unexpected identity: {:?}",
        identity
    );
}

/// Mock binary prints $GEMINI_CLI_HOME to a file, plus writes oauth_creds.json.
/// GEMINI_CLI_HOME in the spawned process should be a scratch dir, not the real home.
#[test]
#[cfg(unix)]
fn gemini_cli_home_set_in_spawned_process() {
    let _g = SPAWN_LOCK
        .lock()
        .unwrap_or_else(|p: std::sync::PoisonError<_>| p.into_inner());
    let tmp = tempdir().unwrap();
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    // Script captures GEMINI_CLI_HOME, then writes credentials there
    let script = "mkdir -p \"$GEMINI_CLI_HOME/.gemini\"\n\
        printf '%s' \"$GEMINI_CLI_HOME\" > \"$GEMINI_CLI_HOME/.gemini/captured_home\"\n\
        printf '{\"token\":\"tok\"}' > \"$GEMINI_CLI_HOME/.gemini/oauth_creds.json\"\n\
        exit 0\n";
    let bin = make_mock_binary(&bin_dir, "gemini", script);

    let ps = ProfileStore::new(tmp.path());
    let cs = ConfigStore::new(tmp.path());
    gemini::add_oauth_with_for_test(
        &ps,
        &cs,
        "work",
        None,
        &bin,
        Duration::from_secs(2),
        TEST_POLL,
    )
    .unwrap();

    // The captured_home file should be in the profile
    let captured_home_bytes = ps
        .read_file(aisw::types::Tool::Gemini, "work", "captured_home")
        .unwrap();
    let captured_home = std::str::from_utf8(&captured_home_bytes).unwrap().trim();

    let real_home = dirs::home_dir().unwrap();
    assert_ne!(
        captured_home,
        real_home.to_str().unwrap(),
        "GEMINI_CLI_HOME should be a scratch dir, not the real home"
    );
    assert!(
        !captured_home.is_empty(),
        "GEMINI_CLI_HOME should not be empty"
    );
}
