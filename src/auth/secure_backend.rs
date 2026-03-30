use anyhow::Result;

use super::system_keyring;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecureBackend {
    SystemKeyring,
}

pub fn read_generic_password(
    backend: SecureBackend,
    service: &str,
    account: Option<&str>,
) -> Result<Option<Vec<u8>>> {
    match backend {
        SecureBackend::SystemKeyring => system_keyring::read_generic_password(service, account),
    }
}

pub fn upsert_generic_password(
    backend: SecureBackend,
    service: &str,
    account: &str,
    secret: &[u8],
) -> Result<()> {
    match backend {
        SecureBackend::SystemKeyring => {
            system_keyring::upsert_generic_password(service, account, secret)
        }
    }
}

pub fn delete_generic_password(backend: SecureBackend, service: &str, account: &str) -> Result<()> {
    match backend {
        SecureBackend::SystemKeyring => system_keyring::delete_generic_password(service, account),
    }
}

pub fn find_generic_password_account(
    backend: SecureBackend,
    service: &str,
) -> Result<Option<String>> {
    match backend {
        SecureBackend::SystemKeyring => system_keyring::find_generic_password_account(service),
    }
}
