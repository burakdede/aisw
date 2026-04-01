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

    Ok(Some(output.stdout))
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

#[cfg(test)]
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
                   printf '{{\"oauthToken\":\"tok\"}}'\n\
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
                   printf 'secret'\n\
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
}
