use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use super::test_overrides;

pub fn read_generic_password(service: &str, account: Option<&str>) -> Result<Option<Vec<u8>>> {
    ensure_available()?;
    let mut command = Command::new(security_bin());
    command.args(["find-generic-password", "-s", service, "-w"]);
    if let Some(account) = account {
        command.args(["-a", account]);
    }
    if let Some(path) = explicit_keychain_path() {
        command.arg(path);
    }

    let output = command
        .output()
        .context("could not query macOS Keychain generic password")?;

    if output.status.success() {
        let mut bytes = output.stdout;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
        }
        return Ok(Some(bytes));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() == Some(44)
        || stderr.contains("could not be found")
        || stderr.contains("not found in the keychain")
    {
        Ok(None)
    } else {
        bail!(
            "could not read macOS Keychain generic password: {}",
            stderr.trim()
        )
    }
}

pub fn upsert_generic_password(service: &str, account: &str, secret: &[u8]) -> Result<()> {
    ensure_available()?;
    let secret = std::str::from_utf8(secret).context("keychain secret is not valid UTF-8")?;

    let mut command = Command::new(security_bin());
    command
        .args([
            "add-generic-password",
            "-U",
            "-a",
            account,
            "-s",
            service,
            "-w",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .context("could not write macOS Keychain generic password")?;
    let mut stdin = child
        .stdin
        .take()
        .context("could not open stdin for macOS Keychain password prompt")?;
    stdin
        .write_all(secret.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .context("could not send secret to macOS Keychain password prompt")?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .context("could not write macOS Keychain generic password")?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "could not update macOS Keychain generic password: {}",
            stderr.trim()
        )
    }
}

pub fn find_generic_password_account(service: &str) -> Result<Option<String>> {
    ensure_available()?;
    let mut command = Command::new(security_bin());
    command.args(["find-generic-password", "-s", service]);
    if let Some(path) = explicit_keychain_path() {
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

pub fn delete_generic_password(service: &str, account: &str) -> Result<()> {
    ensure_available()?;
    let mut command = Command::new(security_bin());
    command.args(["delete-generic-password", "-s", service, "-a", account]);
    if let Some(path) = explicit_keychain_path() {
        command.arg(path);
    }

    let output = command
        .output()
        .context("could not delete macOS Keychain generic password")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() == Some(44)
        || stderr.contains("could not be found")
        || stderr.contains("not found in the keychain")
    {
        Ok(())
    } else {
        bail!(
            "could not delete macOS Keychain generic password: {}",
            stderr.trim()
        )
    }
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

fn explicit_keychain_path() -> Option<PathBuf> {
    if let Some(path) = test_overrides::string("AISW_SECURITY_KEYCHAIN") {
        return Some(PathBuf::from(path));
    }

    login_keychain_path()
}

fn login_keychain_path() -> Option<PathBuf> {
    let output = Command::new(security_bin())
        .args(["login-keychain", "-d", "user"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_first_quoted_value(&combined).map(PathBuf::from)
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
    use tempfile::tempdir;

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
    #[cfg(unix)]
    fn explicit_keychain_path_uses_security_login_keychain() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin = dir.path().join("security");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             if [ \"$1\" = \"login-keychain\" ]; then\n\
               printf '    \"/tmp/test-login.keychain-db\"\\n'\n\
               exit 0\n\
             fi\n\
             exit 1\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let _security = EnvVarGuard::set("AISW_SECURITY_BIN", &bin);
        let _keychain = EnvVarGuard::set("AISW_SECURITY_KEYCHAIN", "");
        std::env::remove_var("AISW_SECURITY_KEYCHAIN");

        assert_eq!(
            explicit_keychain_path(),
            Some(PathBuf::from("/tmp/test-login.keychain-db"))
        );
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
}
