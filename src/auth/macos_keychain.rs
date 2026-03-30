use std::ffi::CStr;
use std::io::Write;
use std::mem::MaybeUninit;
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
    for line in combined.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("\"acct\"") {
            continue;
        }
        if let Some(eq) = trimmed.find('=') {
            let value = trimmed[eq + 1..].trim().trim_matches('"');
            if !value.is_empty() {
                return Ok(Some(value.to_owned()));
            }
        }
    }

    Ok(None)
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

    current_user_home_dir().map(|home| home.join("Library/Keychains/login.keychain-db"))
}

#[cfg(target_os = "macos")]
fn current_user_home_dir() -> Option<PathBuf> {
    let uid = unsafe { libc::geteuid() };
    let size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let mut buf = vec![0u8; if size > 0 { size as usize } else { 4096 }];
    let mut pwd = MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();

    let rc = unsafe {
        libc::getpwuid_r(
            uid,
            pwd.as_mut_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() {
        return None;
    }

    let pwd = unsafe { pwd.assume_init() };
    if pwd.pw_dir.is_null() {
        return None;
    }

    let home = unsafe { CStr::from_ptr(pwd.pw_dir) }.to_str().ok()?;
    Some(PathBuf::from(home))
}

#[cfg(not(target_os = "macos"))]
fn current_user_home_dir() -> Option<PathBuf> {
    None
}
