use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
#[cfg(target_os = "macos")]
use security_framework::passwords;

use super::test_overrides;

pub fn find_generic_password_account(service: &str) -> Result<Option<String>> {
    ensure_available()?;
    let mut command = Command::new(security_bin());
    command.args(["find-generic-password", "-s", service]);
    if let Some(path) = override_keychain_path() {
        command.arg(path);
    }

    let output = command
        .output()
        .context("could not inspect macOS Keychain generic password")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() == Some(44)
            || stderr.contains("could not be found")
            || stderr.contains("not found in the keychain")
        {
            return Ok(None);
        }
        if is_user_canceled(&output.status, &stderr) {
            bail!(
                "Keychain access was denied.\n  \
                 Run the command again and click 'Always Allow' so aisw can manage \
                 credentials without repeated prompts."
            );
        }
        bail!(
            "could not inspect macOS Keychain generic password: {}",
            stderr.trim()
        );
    }

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(parse_attribute_value(&combined, "acct"))
}

pub fn read_generic_password(service: &str, account: Option<&str>) -> Result<Option<Vec<u8>>> {
    ensure_available()?;
    let mut command = Command::new(security_bin());
    command.args(["find-generic-password", "-s", service]);
    if let Some(account) = account {
        command.args(["-a", account]);
    }
    command.arg("-w");
    if let Some(path) = override_keychain_path() {
        command.arg(path);
    }

    let output = command
        .output()
        .context("could not read macOS Keychain generic password")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() == Some(44)
            || stderr.contains("could not be found")
            || stderr.contains("not found in the keychain")
        {
            return Ok(None);
        }
        if is_user_canceled(&output.status, &stderr) {
            bail!(
                "Keychain access was denied.\n  \
                 Run the command again and click 'Always Allow' so aisw can manage \
                 credentials without repeated prompts."
            );
        }
        bail!(
            "could not read macOS Keychain generic password: {}",
            stderr.trim()
        );
    }

    Ok(Some(strip_cli_output_terminator(output.stdout)))
}

/// Removes the single trailing newline `security(1)` writes after a `-w` value.
///
/// That byte is part of the CLI's output framing, not the stored secret, and it
/// cannot be confused with data: `security` switches from text to hex output as
/// soon as the stored value contains a newline anywhere, so a value printed as
/// text provably has none of its own. Exactly one byte comes off — a trailing
/// space *is* data and survives text mode intact, and the hex form legitimately
/// decodes to bytes that end in a newline.
///
/// Leaving it on meant aisw stored the terminator and wrote it back verbatim
/// through `set_generic_password`, which pushed the Keychain item into the hex
/// form that Claude Code's own reader cannot parse (#250).
fn strip_cli_output_terminator(mut stdout: Vec<u8>) -> Vec<u8> {
    if stdout.last() == Some(&b'\n') {
        stdout.pop();
    }
    stdout
}

pub fn upsert_generic_password(
    service: &str,
    account: &str,
    secret: &[u8],
    trusted_apps: &[PathBuf],
) -> Result<()> {
    ensure_available()?;

    if test_overrides::var("AISW_SECURITY_BIN").is_none()
        && test_overrides::var("AISW_SECURITY_KEYCHAIN").is_none()
    {
        #[cfg(target_os = "macos")]
        {
            let _ = trusted_apps;
            return passwords::set_generic_password(service, account, secret)
                .context("could not update macOS Keychain generic password");
        }
    }

    let mut command = Command::new(security_bin());
    command.args(["add-generic-password", "-U", "-s", service, "-a", account]);
    for app in trusted_apps {
        if let Some(path) = app.to_str() {
            command.args(["-T", path]);
        }
    }
    command.arg("-w");
    command.stdin(Stdio::piped());

    let mut child = command
        .spawn()
        .context("could not update macOS Keychain generic password")?;

    {
        let mut stdin = child
            .stdin
            .take()
            .context("could not open stdin for macOS Keychain update")?;
        stdin
            .write_all(secret)
            .context("could not write macOS Keychain secret")?;
        stdin
            .write_all(b"\n")
            .context("could not finalize macOS Keychain secret write")?;
    }

    let output = child
        .wait_with_output()
        .context("could not wait for macOS Keychain update")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_user_canceled(&output.status, &stderr) {
        bail!(
            "Keychain access was denied.\n  \
             Run the command again and click 'Always Allow' so aisw can manage \
             credentials without repeated prompts."
        );
    }
    bail!(
        "could not update macOS Keychain generic password: {}",
        stderr.trim()
    );
}

#[cfg(any(target_os = "macos", all(test, unix)))]
pub(crate) fn upsert_go_password(service: &str, account: &str, secret: &[u8]) -> Result<()> {
    ensure_available()?;
    let secret = std::str::from_utf8(secret).context("Go Keychain secret is not valid UTF-8")?;
    let quote = |value: &str| -> Result<String> {
        if value.chars().any(char::is_control) {
            bail!("Go Keychain command values must contain no control characters");
        }
        Ok(format!(
            "\"{}\"",
            value.replace('\\', "\\\\").replace('\"', "\\\"")
        ))
    };
    let mut line = format!(
        "add-generic-password -U -s {} -a {} -w {}",
        quote(service)?,
        quote(account)?,
        quote(secret)?
    );
    if let Some(path) = override_keychain_path() {
        if !path.is_file() {
            bail!("explicit Go Keychain path must name an existing file");
        }
        line.push(' ');
        line.push_str(&quote(
            path.to_str().context("Keychain path is not valid UTF-8")?,
        )?);
    }
    line.push('\n');
    if line.len() > 4096 {
        bail!("Go Keychain credential exceeds the upstream 4096-byte command limit");
    }
    // Go reads through security; native updates revoke that reader's partition.
    let mut child = Command::new(
        test_overrides::string("AISW_SECURITY_BIN")
            .unwrap_or_else(|| "/usr/bin/security".to_owned()),
    )
    .arg("-i")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .context("could not start Go Keychain update")?;
    child
        .stdin
        .take()
        .context("could not open Go Keychain update stdin")?
        .write_all(line.as_bytes())
        .context("could not send Go Keychain update")?;
    let output = child
        .wait_with_output()
        .context("could not wait for Go Keychain update")?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_user_canceled(&output.status, &stderr) {
        bail!("Keychain access was denied. Run the command again and click 'Always Allow'.");
    }
    bail!(
        "could not update Go Keychain credential (exit {:?})",
        output.status.code()
    );
}

pub fn is_available() -> bool {
    cfg!(target_os = "macos")
        || test_overrides::var("AISW_SECURITY_BIN").is_some()
        || test_overrides::var("AISW_SECURITY_KEYCHAIN").is_some()
}

fn ensure_available() -> Result<()> {
    if is_available() {
        Ok(())
    } else {
        bail!("macOS Keychain support is only available on macOS")
    }
}

fn security_bin() -> String {
    test_overrides::string("AISW_SECURITY_BIN").unwrap_or_else(|| "security".to_owned())
}

fn override_keychain_path() -> Option<PathBuf> {
    test_overrides::string("AISW_SECURITY_KEYCHAIN").map(PathBuf::from)
}

/// Returns true when the `security` CLI was denied by the user (clicked "Deny"
/// or cancelled the Keychain authorisation dialog).
///
/// The CLI exits with code 128 and/or emits a stderr message containing
/// "User canceled" (note: macOS spells "canceled" with one 'l').
fn is_user_canceled(status: &std::process::ExitStatus, stderr: &str) -> bool {
    status.code() == Some(128) || stderr.contains("User canceled")
}

fn parse_first_quoted_value(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let rest = &text[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn parse_attribute_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(&format!("\"{key}\"")) {
            continue;
        }
        let (_, value) = trimmed.split_once('=')?;
        let value = value.trim();
        if let Some(parsed) = parse_first_quoted_value(value) {
            return Some(parsed);
        }
        if !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use tempfile::tempdir;

    fn exit_status(code: i32) -> std::process::ExitStatus {
        // Portable way to construct an ExitStatus with a known code.
        Command::new("sh")
            .args(["-c", &format!("exit {code}")])
            .status()
            .unwrap()
    }

    #[test]
    fn is_user_canceled_detects_exit_128() {
        assert!(is_user_canceled(&exit_status(128), "some other message"));
    }

    #[test]
    fn is_user_canceled_detects_stderr_message() {
        assert!(is_user_canceled(
            &exit_status(1),
            "User canceled the operation."
        ));
    }

    #[test]
    fn is_user_canceled_returns_false_for_other_errors() {
        assert!(!is_user_canceled(
            &exit_status(1),
            "SecKeychainAddGenericPassword: item already exists"
        ));
        assert!(!is_user_canceled(
            &exit_status(44),
            "could not be found in the keychain"
        ));
    }

    struct EnvVarGuard {
        key: &'static str,
        old: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let old = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, old }
        }

        fn unset(key: &'static str) -> Self {
            let old = std::env::var_os(key);
            std::env::remove_var(key);
            Self { key, old }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.old {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn write_mock_security(bin: &std::path::Path, script: &str) {
        fs::write(bin, script).unwrap();
        fs::set_permissions(bin, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn is_available_is_true_with_security_override() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(&bin, "#!/bin/sh\nexit 0\n");
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        assert!(is_available());
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn ensure_available_paths_error_without_overrides() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _security = EnvVarGuard::unset("AISW_SECURITY_BIN");
        let _keychain = EnvVarGuard::unset("AISW_SECURITY_KEYCHAIN");
        let err = read_generic_password("aisw", None).unwrap_err();
        assert!(err.to_string().contains("only available on macOS"));
    }

    #[test]
    fn find_generic_password_account_parses_blob_output() {
        let output = "keychain: \"/tmp/test-login.keychain-db\"\n\
                      class: \"genp\"\n\
                      attributes:\n\
                          0x00000007 <blob>=\"Codex Auth\"\n\
                          \"acct\"<blob>=\"burak\"\n";

        assert_eq!(
            parse_attribute_value(output, "acct"),
            Some("burak".to_owned())
        );
    }

    #[test]
    #[cfg(unix)]
    fn read_generic_password_uses_account_when_provided() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        let marker = dir.path().join("args");
        fs::write(
            &bin,
            format!(
                "#!/bin/sh\n\
                 printf '%s ' \"$@\" > \"{}\"\n\
                 if [ \"$1\" = \"find-generic-password\" ] && [ \"$2\" = \"-s\" ] && [ \"$3\" = \"Claude Code-credentials\" ] && [ \"$4\" = \"-a\" ] && [ \"$5\" = \"tester\" ] && [ \"$6\" = \"-w\" ]; then\n\
                   printf '{{\"oauthToken\":\"tok\"}}\\n'\n\
                   exit 0\n\
                 fi\n\
                 exit 1\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);

        let bytes = read_generic_password("Claude Code-credentials", Some("tester"))
            .unwrap()
            .expect("password");
        assert_eq!(bytes, br#"{"oauthToken":"tok"}"#);
        assert_eq!(
            fs::read_to_string(marker).unwrap(),
            "find-generic-password -s Claude Code-credentials -a tester -w "
        );
    }

    #[test]
    fn strip_cli_output_terminator_removes_exactly_one_newline() {
        assert_eq!(strip_cli_output_terminator(b"{}\n".to_vec()), b"{}");
        // The hex form decodes to bytes that may legitimately end in a
        // newline, so only the CLI's own terminator comes off.
        assert_eq!(strip_cli_output_terminator(b"{}\n\n".to_vec()), b"{}\n");
        // A trailing space survives `security`'s text mode, so it is data.
        assert_eq!(strip_cli_output_terminator(b"{} \n".to_vec()), b"{} ");
        assert_eq!(strip_cli_output_terminator(b"{}".to_vec()), b"{}");
        assert_eq!(strip_cli_output_terminator(Vec::new()), b"");
        assert_eq!(strip_cli_output_terminator(b"\n".to_vec()), b"");
        // Binary payloads must survive untouched apart from the terminator.
        assert_eq!(
            strip_cli_output_terminator(b"\x00\xff\n".to_vec()),
            b"\x00\xff"
        );
    }

    /// The real `security -w` always terminates its output with a newline.
    /// Keeping it meant aisw wrote it back into the Keychain item, which
    /// forced the hex form Claude Code cannot read (#250).
    #[test]
    #[cfg(unix)]
    fn read_generic_password_strips_the_cli_output_terminator() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\nprintf '{\"claudeAiOauth\":{\"accessToken\":\"tok\"}}\\n'\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);

        let bytes = read_generic_password("Claude Code-credentials", None)
            .unwrap()
            .expect("password");
        assert_eq!(bytes, br#"{"claudeAiOauth":{"accessToken":"tok"}}"#);
    }

    /// Guards against "simplifying" the strip into a `trim_ascii_end`: a
    /// trailing space is preserved by `security`'s text mode, so it is part
    /// of the secret and discarding it would silently corrupt the value.
    #[test]
    #[cfg(unix)]
    fn read_generic_password_preserves_trailing_space_in_the_value() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(&bin, "#!/bin/sh\nprintf 'secret \\n'\n");
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);

        let bytes = read_generic_password("aisw", None)
            .unwrap()
            .expect("password");
        assert_eq!(bytes, b"secret ");
    }

    /// An empty stored value must stay distinguishable from a missing item:
    /// `Some(vec![])`, never `None`, so callers do not treat a wiped secret
    /// as "no profile configured" and skip the write that would repair it.
    #[test]
    #[cfg(unix)]
    fn read_generic_password_reports_an_empty_value_as_present() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(&bin, "#!/bin/sh\nprintf '\\n'\n");
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);

        assert_eq!(
            read_generic_password("Claude Code-credentials", None).unwrap(),
            Some(Vec::new())
        );
    }

    /// `security` emits hex whenever the stored value contains a newline. The
    /// reader must hand that form through byte-for-byte; decoding it is the
    /// credential layer's job.
    #[test]
    #[cfg(unix)]
    fn read_generic_password_passes_the_hex_form_through_unchanged() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\nprintf '7b226f61757468546f6b656e223a22746f6b227d0a\\n'\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);

        let bytes = read_generic_password("Claude Code-credentials", None)
            .unwrap()
            .expect("password");
        assert_eq!(bytes, b"7b226f61757468546f6b656e223a22746f6b227d0a");
    }

    #[test]
    fn read_generic_password_returns_none_on_not_found() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\n\
             echo 'security: item could not be found in the keychain' >&2\n\
             exit 44\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let result = read_generic_password("Claude Code-credentials", Some("missing")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_generic_password_surfaces_canceled_error() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\n\
             echo 'User canceled.' >&2\n\
             exit 128\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let err = read_generic_password("Claude Code-credentials", Some("tester")).unwrap_err();
        assert!(err.to_string().contains("Keychain access was denied"));
    }

    #[test]
    fn read_generic_password_surfaces_generic_error() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\n\
             echo 'unexpected failure' >&2\n\
             exit 1\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let err = read_generic_password("Claude Code-credentials", Some("tester")).unwrap_err();
        assert!(err
            .to_string()
            .contains("could not read macOS Keychain generic password"));
    }

    #[test]
    fn find_generic_password_account_returns_none_on_not_found() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\n\
             echo 'security: item could not be found in the keychain' >&2\n\
             exit 44\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let account = find_generic_password_account("aisw").unwrap();
        assert!(account.is_none());
    }

    #[test]
    fn find_generic_password_account_surfaces_generic_error() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\n\
             echo 'parse blew up' >&2\n\
             exit 1\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let err = find_generic_password_account("aisw").unwrap_err();
        assert!(err
            .to_string()
            .contains("could not inspect macOS Keychain generic password"));
    }

    /// Pins the `security(1)` behaviour the reader is written against, using
    /// the real binary rather than a stub.
    ///
    /// Every other test here mocks `security`, and a mock cannot model the one
    /// thing that mattered in #250: the CLI switches from text to hex output
    /// the moment the stored value contains a newline, and that switch happens
    /// inside the binary. If a future macOS changes the framing or the
    /// threshold for the hex form, the fix's central assumption — that a
    /// text-mode value provably has no newline of its own — stops holding, and
    /// this is what says so.
    ///
    /// Hermetic: a throwaway keychain in a temp dir, never added to the search
    /// list, so the login keychain is not consulted and no real credential is
    /// touched. The item is both written and read by `security` itself, so
    /// there is no cross-application access check to prompt for.
    #[test]
    #[cfg(target_os = "macos")]
    fn real_security_cli_output_framing_matches_the_reader() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let keychain = dir.path().join("aisw-test.keychain-db");

        let created = Command::new("security")
            .args(["create-keychain", "-p", "aisw-test"])
            .arg(&keychain)
            .output()
            .expect("security should be available on macOS");
        assert!(
            created.status.success(),
            "could not create the throwaway keychain: {}",
            String::from_utf8_lossy(&created.stderr)
        );
        let _keychain_cleanup = ThrowawayKeychain(keychain.clone());

        let unlocked = Command::new("security")
            .args(["unlock-keychain", "-p", "aisw-test"])
            .arg(&keychain)
            .output()
            .expect("security should be available on macOS");
        assert!(unlocked.status.success(), "could not unlock the keychain");

        let _bin = EnvVarGuard::unset("AISW_SECURITY_BIN");
        let _path = EnvVarGuard::set("AISW_SECURITY_KEYCHAIN", &keychain);

        // Text form: the CLI appends a terminator the reader has to drop, and
        // a trailing space inside the value survives, which is why only the
        // final newline may come off.
        store_exact(&keychain, "clean", b"{\"oauthToken\":\"tok\"} ");
        assert_eq!(
            read_generic_password("clean", Some("tester"))
                .unwrap()
                .unwrap(),
            b"{\"oauthToken\":\"tok\"} ",
            "the reader must strip the CLI terminator and nothing else"
        );

        // Hex form: this is the state #250 left the Keychain in. The reader
        // hands it through, and the credential layer decodes it back to the
        // original bytes, trimming only the edges.
        let with_newline = b"{\"oauthToken\":\"tok\"}\n";
        store_exact(&keychain, "newline", with_newline);
        let raw = read_generic_password("newline", Some("tester"))
            .unwrap()
            .unwrap();
        assert!(
            raw.iter().all(u8::is_ascii_hexdigit),
            "security should print a value containing a newline as hex, got: {}",
            String::from_utf8_lossy(&raw)
        );
        assert_eq!(
            crate::auth::claude::normalize_credentials_bytes(&raw).unwrap(),
            br#"{"oauthToken":"tok"}"#,
            "decoding the hex form must recover the payload without the newline"
        );

        use crate::config::CredentialBackend;
        use crate::profile::ProfileStore;
        use crate::types::Tool;
        use base64::{engine::general_purpose::STANDARD, Engine};

        let encoded = format!(
            "go-keyring-base64:{}",
            STANDARD.encode(br#"{"email":"work@example.com","token":"synthetic"}"#)
        );
        let quoted = br#"{"value":"O'Reilly $(touch should-not-exist) `uname`"}"#;
        upsert_go_password("Go's fixture", "tester", quoted).unwrap();
        assert_eq!(
            read_generic_password("Go's fixture", Some("tester"))
                .unwrap()
                .unwrap(),
            quoted
        );

        let profiles = ProfileStore::new(&dir.path().join("managed"));
        profiles.create(Tool::Antigravity, "work").unwrap();
        for suffix in ["", "\n"] {
            store_exact(
                &keychain,
                "antigravity-fixture",
                format!("{encoded}{suffix}").as_bytes(),
            );
            let captured = read_generic_password("antigravity-fixture", Some("tester"))
                .unwrap()
                .unwrap();
            crate::auth::antigravity::persist_managed_secret(
                &profiles,
                "work",
                CredentialBackend::File,
                &captured,
            )
            .unwrap();
            let restored = crate::auth::antigravity::read_managed_secret(
                &profiles,
                "work",
                CredentialBackend::File,
            )
            .unwrap()
            .unwrap();
            upsert_go_password("antigravity-fixture", "tester", &restored).unwrap();
            let upstream = read_generic_password("antigravity-fixture", Some("tester"))
                .unwrap()
                .unwrap();
            assert_eq!(upstream, encoded.as_bytes());
            assert_eq!(
                STANDARD
                    .decode(upstream.strip_prefix(b"go-keyring-base64:").unwrap())
                    .unwrap(),
                br#"{"email":"work@example.com","token":"synthetic"}"#
            );
        }

        for payload in [
            b"{\n  \"oauthToken\": \"synthetic\"\n}".as_slice(),
            "{\"oauthToken\":\"synthetic\",\"name\":\"Gödel 👋\"}".as_bytes(),
        ] {
            store_exact(&keychain, "claude-format-fixture", payload);
            let captured = read_generic_password("claude-format-fixture", Some("tester"))
                .unwrap()
                .unwrap();
            let restored = crate::auth::claude::normalize_credentials_bytes(&captured).unwrap();
            store_exact(&keychain, "claude-format-fixture", &restored);
            let upstream = read_generic_password("claude-format-fixture", Some("tester"))
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&upstream).unwrap(),
                serde_json::from_slice::<serde_json::Value>(payload).unwrap()
            );
        }

        // An empty stored value must stay distinguishable from a missing item.
        assert!(read_generic_password("absent", Some("tester"))
            .unwrap()
            .is_none());
    }

    /// Writes exact bytes into the throwaway keychain via `-X`, so the test
    /// controls the stored value down to the byte rather than going through
    /// the CLI's line-oriented password prompt.
    #[cfg(target_os = "macos")]
    fn store_exact(keychain: &std::path::Path, service: &str, value: &[u8]) {
        let hex: String = value.iter().map(|byte| format!("{byte:02x}")).collect();
        let output = Command::new("security")
            .args([
                "add-generic-password",
                "-U",
                "-a",
                "tester",
                "-s",
                service,
                "-X",
                &hex,
            ])
            .arg(keychain)
            .output()
            .expect("security should be available on macOS");
        assert!(
            output.status.success(),
            "could not store the {service} fixture: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Deletes the throwaway keychain on drop, so a failed assertion does not
    /// strand it.
    #[cfg(target_os = "macos")]
    struct ThrowawayKeychain(std::path::PathBuf);

    #[cfg(target_os = "macos")]
    impl Drop for ThrowawayKeychain {
        fn drop(&mut self) {
            let _ = Command::new("security")
                .arg("delete-keychain")
                .arg(&self.0)
                .output();
        }
    }

    #[test]
    #[cfg(unix)]
    fn read_generic_password_uses_explicit_keychain_path_for_aisw_service() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        let marker = dir.path().join("args");
        fs::write(
            &bin,
            format!(
                "#!/bin/sh\n\
                 printf '%s ' \"$@\" > \"{}\"\n\
                 if [ \"$1\" = \"find-generic-password\" ] && [ \"$2\" = \"-s\" ] && [ \"$3\" = \"aisw\" ] && [ \"$4\" = \"-a\" ] && [ \"$5\" = \"profile:claude:default\" ] && [ \"$6\" = \"-w\" ]; then\n\
                   printf 'secret\\n'\n\
                   exit 0\n\
                 fi\n\
                 exit 1\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);

        let bytes = read_generic_password("aisw", Some("profile:claude:default"))
            .unwrap()
            .expect("password");
        assert_eq!(bytes, b"secret");
        assert_eq!(
            fs::read_to_string(marker).unwrap(),
            "find-generic-password -s aisw -a profile:claude:default -w "
        );
    }

    #[test]
    #[cfg(unix)]
    fn upsert_generic_password_writes_secret_via_stdin_and_trusts_apps() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        let marker = dir.path().join("args");
        let stdin_capture = dir.path().join("stdin");
        fs::write(
            &bin,
            format!(
                "#!/bin/sh\n\
                 printf '%s ' \"$@\" > \"{}\"\n\
                 cat > \"{}\"\n\
                 exit 0\n",
                marker.display(),
                stdin_capture.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let trusted = dir.path().join("claude");
        fs::write(&trusted, "").unwrap();
        fs::set_permissions(&trusted, fs::Permissions::from_mode(0o755)).unwrap();

        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);

        upsert_generic_password(
            "Claude Code-credentials",
            "tester",
            br#"{"claudeAiOauth":{"accessToken":"tok"}}"#,
            std::slice::from_ref(&trusted),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(marker).unwrap(),
            format!(
                "add-generic-password -U -s Claude Code-credentials -a tester -T {} -w ",
                trusted.display()
            )
        );
        assert_eq!(
            fs::read_to_string(stdin_capture).unwrap(),
            "{\"claudeAiOauth\":{\"accessToken\":\"tok\"}}\n"
        );
    }

    #[test]
    fn go_keychain_update_uses_interactive_stdin_and_redacts_errors() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        let marker = dir.path().join("args");
        let stdin_capture = dir.path().join("stdin");
        write_mock_security(
            &bin,
            &format!(
                "#!/bin/sh\nprintf '%s' \"$*\" > '{}'\ncat > '{}'\nexit 0\n",
                marker.display(),
                stdin_capture.display()
            ),
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let _path = EnvVarGuard::unset("AISW_SECURITY_KEYCHAIN");
        upsert_go_password("gemini", "antigravity", b"synthetic's token").unwrap();
        assert_eq!(fs::read_to_string(&marker).unwrap(), "-i");
        assert_eq!(
            fs::read_to_string(&stdin_capture).unwrap(),
            "add-generic-password -U -s \"gemini\" -a \"antigravity\" -w \"synthetic's token\"\n"
        );
        write_mock_security(
            &bin,
            "#!/bin/sh\ncat >/dev/null\necho synthetic-secret >&2\nexit 1\n",
        );
        let error = upsert_go_password("gemini", "antigravity", b"synthetic-secret").unwrap_err();
        assert!(!error.to_string().contains("synthetic-secret"));
        assert!(error.to_string().contains("could not update Go Keychain"));
        write_mock_security(
            &bin,
            "#!/bin/sh\ncat >/dev/null\necho 'User canceled.' >&2\nexit 128\n",
        );
        assert!(upsert_go_password("gemini", "antigravity", b"secret")
            .unwrap_err()
            .to_string()
            .contains("Keychain access was denied"));
    }

    #[test]
    fn go_keychain_update_rejects_command_injection_before_spawn() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", "/missing/security");
        let _path = EnvVarGuard::unset("AISW_SECURITY_KEYCHAIN");
        for value in [
            b"secret\ndelete-generic-password".as_slice(),
            b"secret\r",
            b"secret\0",
        ] {
            assert!(upsert_go_password("gemini", "antigravity", value)
                .unwrap_err()
                .to_string()
                .contains("control characters"));
        }
        assert!(upsert_go_password("gemini", "antigravity", &[0xff])
            .unwrap_err()
            .to_string()
            .contains("UTF-8"));
        assert!(
            upsert_go_password("gemini", "antigravity", &vec![b'x'; 4096])
                .unwrap_err()
                .to_string()
                .contains("4096-byte")
        );
    }

    #[test]
    fn upsert_generic_password_surfaces_canceled_error() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\n\
             cat >/dev/null\n\
             echo 'User canceled.' >&2\n\
             exit 128\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let err = upsert_generic_password("aisw", "acct", b"secret", &[]).unwrap_err();
        assert!(err.to_string().contains("Keychain access was denied"));
    }

    #[test]
    fn upsert_generic_password_surfaces_generic_error() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        write_mock_security(
            &bin,
            "#!/bin/sh\n\
             cat >/dev/null\n\
             echo 'boom' >&2\n\
             exit 1\n",
        );
        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let err = upsert_generic_password("aisw", "acct", b"secret", &[]).unwrap_err();
        assert!(err
            .to_string()
            .contains("could not update macOS Keychain generic password"));
    }
}
