use anyhow::Result;

use super::macos_keychain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecureBackend {
    MacosKeychain,
}

pub fn read_generic_password(
    backend: SecureBackend,
    service: &str,
    account: Option<&str>,
) -> Result<Option<Vec<u8>>> {
    match backend {
        SecureBackend::MacosKeychain => macos_keychain::read_generic_password(service, account),
    }
}

pub fn upsert_generic_password(
    backend: SecureBackend,
    service: &str,
    account: &str,
    secret: &[u8],
) -> Result<()> {
    match backend {
        SecureBackend::MacosKeychain => {
            macos_keychain::upsert_generic_password(service, account, secret)
        }
    }
}

pub fn delete_generic_password(backend: SecureBackend, service: &str, account: &str) -> Result<()> {
    match backend {
        SecureBackend::MacosKeychain => macos_keychain::delete_generic_password(service, account),
    }
}

pub fn find_generic_password_account(
    backend: SecureBackend,
    service: &str,
) -> Result<Option<String>> {
    match backend {
        SecureBackend::MacosKeychain => macos_keychain::find_generic_password_account(service),
    }
}
