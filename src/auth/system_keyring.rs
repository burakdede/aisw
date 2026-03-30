use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use super::macos_keychain;
use super::test_overrides;

pub fn read_generic_password(service: &str, account: Option<&str>) -> Result<Option<Vec<u8>>> {
    if let Some(root) = fake_root() {
        return read_fake_password(&root, service, account);
    }

    let Some(account) = resolve_account(service, account)? else {
        return Ok(None);
    };
    let entry = keyring::Entry::new(service, &account).map_err(|err| {
        anyhow!("could not open system keyring entry for {service}/{account}: {err}")
    })?;

    match entry.get_password() {
        Ok(secret) => Ok(Some(secret.into_bytes())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(anyhow!(
            "could not read system keyring entry for {service}/{account}: {err}"
        )),
    }
}

pub fn upsert_generic_password(service: &str, account: &str, secret: &[u8]) -> Result<()> {
    if let Some(root) = fake_root() {
        return write_fake_password(&root, service, account, secret);
    }

    let secret = std::str::from_utf8(secret).context("keyring secret is not valid UTF-8")?;
    let entry = keyring::Entry::new(service, account).map_err(|err| {
        anyhow!("could not open system keyring entry for {service}/{account}: {err}")
    })?;
    entry.set_password(secret).map_err(|err| {
        anyhow!("could not write system keyring entry for {service}/{account}: {err}")
    })
}

pub fn delete_generic_password(service: &str, account: &str) -> Result<()> {
    if let Some(root) = fake_root() {
        return delete_fake_password(&root, service, account);
    }

    let entry = keyring::Entry::new(service, account).map_err(|err| {
        anyhow!("could not open system keyring entry for {service}/{account}: {err}")
    })?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(anyhow!(
            "could not delete system keyring entry for {service}/{account}: {err}"
        )),
    }
}

pub fn find_generic_password_account(service: &str) -> Result<Option<String>> {
    if let Some(root) = fake_root() {
        return find_fake_account(&root, service);
    }

    if cfg!(target_os = "macos") {
        return macos_keychain::find_generic_password_account(service);
    }

    let Some(account) = current_username() else {
        return Ok(None);
    };
    Ok(read_generic_password(service, Some(&account))?.map(|_| account))
}

fn resolve_account(service: &str, account: Option<&str>) -> Result<Option<String>> {
    if let Some(account) = account {
        return Ok(Some(account.to_owned()));
    }
    find_generic_password_account(service)
}

fn fake_root() -> Option<PathBuf> {
    test_overrides::string("AISW_KEYRING_TEST_DIR").map(PathBuf::from)
}

fn fake_item_dir(root: &Path, service: &str, account: &str) -> PathBuf {
    root.join(service).join(account)
}

fn read_fake_password(
    root: &Path,
    service: &str,
    account: Option<&str>,
) -> Result<Option<Vec<u8>>> {
    let Some(account) = (match account {
        Some(account) => Some(account.to_owned()),
        None => find_fake_account(root, service)?,
    }) else {
        return Ok(None);
    };
    let path = fake_item_dir(root, service, &account).join("secret");
    if !path.exists() {
        return Ok(None);
    }
    fs::read(&path)
        .with_context(|| format!("could not read {}", path.display()))
        .map(Some)
}

fn write_fake_password(root: &Path, service: &str, account: &str, secret: &[u8]) -> Result<()> {
    let item_dir = fake_item_dir(root, service, account);
    fs::create_dir_all(&item_dir)
        .with_context(|| format!("could not create {}", item_dir.display()))?;
    fs::write(item_dir.join("account"), account.as_bytes())
        .with_context(|| format!("could not write {}/account", item_dir.display()))?;
    fs::write(item_dir.join("secret"), secret)
        .with_context(|| format!("could not write {}/secret", item_dir.display()))
}

fn delete_fake_password(root: &Path, service: &str, account: &str) -> Result<()> {
    let item_dir = fake_item_dir(root, service, account);
    if !item_dir.exists() {
        return Ok(());
    }
    fs::remove_dir_all(&item_dir)
        .with_context(|| format!("could not delete {}", item_dir.display()))
}

fn find_fake_account(root: &Path, service: &str) -> Result<Option<String>> {
    let service_dir = root.join(service);
    if !service_dir.exists() {
        return Ok(None);
    }

    let mut accounts = Vec::new();
    for entry in fs::read_dir(&service_dir)
        .with_context(|| format!("could not read {}", service_dir.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            accounts.push(entry.file_name());
        }
    }
    accounts.sort();
    let Some(account) = accounts.into_iter().next() else {
        return Ok(None);
    };
    account
        .into_string()
        .map(Some)
        .map_err(|_| anyhow!("keyring account name for {} is not valid UTF-8", service))
}

fn current_username() -> Option<String> {
    std::env::var("USER")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("USERNAME")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .or_else(current_username_from_os)
}

#[cfg(unix)]
fn current_username_from_os() -> Option<String> {
    let uid = unsafe { libc::geteuid() };
    let size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let mut buf = vec![0u8; if size > 0 { size as usize } else { 4096 }];
    let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
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
    if pwd.pw_name.is_null() {
        return None;
    }

    unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) }
        .to_str()
        .ok()
        .map(ToOwned::to_owned)
}

#[cfg(not(unix))]
fn current_username_from_os() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
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
    fn fake_keyring_round_trip_and_find_account() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let _root = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path());

        upsert_generic_password("aisw", "profile:codex:work", br#"{"token":"tok"}"#).unwrap();

        assert_eq!(
            read_generic_password("aisw", Some("profile:codex:work"))
                .unwrap()
                .as_deref(),
            Some(br#"{"token":"tok"}"#.as_slice())
        );
        assert_eq!(
            find_generic_password_account("aisw").unwrap(),
            Some("profile:codex:work".to_owned())
        );
    }

    #[test]
    fn fake_keyring_delete_is_idempotent() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let _root = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path());

        delete_generic_password("aisw", "missing").unwrap();
    }
}
