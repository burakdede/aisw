use anyhow::{bail, Result};

use super::secure_backend::{self, SecureBackend};
use crate::types::Tool;

const SERVICE: &str = "aisw";
const BACKEND: SecureBackend = SecureBackend::SystemKeyring;

fn enrich_system_keyring_error(err: anyhow::Error) -> anyhow::Error {
    if let Some(diagnostic) = super::system_keyring::usability_diagnostic() {
        return err.context(diagnostic);
    }
    err
}

pub fn read_profile_secret(tool: Tool, profile_name: &str) -> Result<Option<Vec<u8>>> {
    secure_backend::read_generic_password(
        BACKEND,
        SERVICE,
        Some(&profile_account(tool, profile_name)),
    )
    .map_err(enrich_system_keyring_error)
}

pub fn write_profile_secret(tool: Tool, profile_name: &str, bytes: &[u8]) -> Result<()> {
    secure_backend::upsert_generic_password(
        BACKEND,
        SERVICE,
        &profile_account(tool, profile_name),
        bytes,
    )
    .map_err(enrich_system_keyring_error)
}

pub fn delete_profile_secret(tool: Tool, profile_name: &str) -> Result<()> {
    secure_backend::delete_generic_password(BACKEND, SERVICE, &profile_account(tool, profile_name))
        .map_err(enrich_system_keyring_error)
}

pub fn rename_profile_secret(tool: Tool, old_name: &str, new_name: &str) -> Result<()> {
    let Some(bytes) = read_profile_secret(tool, old_name)? else {
        bail!(
            "secure credentials for {} profile '{}' are missing from the system keyring",
            tool,
            old_name
        );
    };
    write_profile_secret(tool, new_name, &bytes)?;
    delete_profile_secret(tool, old_name)
}

pub fn snapshot_profile_secret(tool: Tool, profile_name: &str, backup_id: &str) -> Result<()> {
    let Some(bytes) = read_profile_secret(tool, profile_name)? else {
        bail!(
            "secure credentials for {} profile '{}' are missing from the system keyring",
            tool,
            profile_name
        );
    };
    secure_backend::upsert_generic_password(
        BACKEND,
        SERVICE,
        &backup_account(tool, profile_name, backup_id),
        &bytes,
    )
    .map_err(enrich_system_keyring_error)
}

pub fn restore_profile_secret(tool: Tool, profile_name: &str, backup_id: &str) -> Result<()> {
    let Some(bytes) = secure_backend::read_generic_password(
        BACKEND,
        SERVICE,
        Some(&backup_account(tool, profile_name, backup_id)),
    )
    .map_err(enrich_system_keyring_error)?
    else {
        bail!(
            "backup '{}' is missing secure credentials for {} profile '{}'",
            backup_id,
            tool,
            profile_name
        );
    };
    write_profile_secret(tool, profile_name, &bytes)
}

pub fn delete_backup_secret(tool: Tool, profile_name: &str, backup_id: &str) -> Result<()> {
    secure_backend::delete_generic_password(
        BACKEND,
        SERVICE,
        &backup_account(tool, profile_name, backup_id),
    )
    .map_err(enrich_system_keyring_error)
}

fn profile_account(tool: Tool, profile_name: &str) -> String {
    format!("profile:{}:{}", tool.binary_name(), profile_name)
}

fn backup_account(tool: Tool, profile_name: &str, backup_id: &str) -> String {
    format!(
        "backup:{}:{}:{}",
        backup_id,
        tool.binary_name(),
        profile_name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::tempdir;

    struct CanaryCleanup {
        accounts: Vec<String>,
    }

    impl CanaryCleanup {
        fn new() -> Self {
            Self {
                accounts: Vec::new(),
            }
        }

        fn track_profile(&mut self, tool: Tool, profile_name: &str) {
            self.accounts.push(profile_account(tool, profile_name));
        }

        fn track_backup(&mut self, tool: Tool, profile_name: &str, backup_id: &str) {
            self.accounts
                .push(backup_account(tool, profile_name, backup_id));
        }
    }

    impl Drop for CanaryCleanup {
        fn drop(&mut self) {
            for account in &self.accounts {
                let _ = secure_backend::delete_generic_password(BACKEND, SERVICE, account);
            }
        }
    }

    fn canary_profile_name() -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after UNIX_EPOCH")
            .as_millis();
        format!("canary-{}-{}", std::process::id(), millis)
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn profile_and_backup_account_formats_are_stable() {
        assert_eq!(profile_account(Tool::Claude, "work"), "profile:claude:work");
        assert_eq!(
            backup_account(Tool::Codex, "main", "bkp-1"),
            "backup:bkp-1:codex:main"
        );
    }

    #[test]
    fn rename_profile_secret_errors_when_source_secret_missing() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path());

        let err = rename_profile_secret(Tool::Codex, "missing", "new-name").unwrap_err();
        assert!(err.to_string().contains("missing from the system keyring"));
    }

    #[test]
    fn snapshot_profile_secret_errors_when_source_secret_missing() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path());

        let err = snapshot_profile_secret(Tool::Claude, "missing", "bkp").unwrap_err();
        assert!(err.to_string().contains("missing from the system keyring"));
    }

    #[test]
    fn restore_profile_secret_errors_when_backup_secret_missing() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path());

        let err = restore_profile_secret(Tool::Gemini, "work", "bkp").unwrap_err();
        assert!(err.to_string().contains("is missing secure credentials"));
    }

    #[test]
    #[cfg(not(windows))]
    fn snapshot_restore_and_delete_backup_secret_round_trip_in_fake_keyring() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let _keyring = EnvVarGuard::set("AISW_KEYRING_TEST_DIR", dir.path());

        let tool = Tool::Codex;
        let profile = "work";
        let backup_id = "backup-123";
        let original = br#"{"token":"tok-original"}"#;
        let replacement = br#"{"token":"tok-replacement"}"#;

        write_profile_secret(tool, profile, original).unwrap();
        snapshot_profile_secret(tool, profile, backup_id).unwrap();
        write_profile_secret(tool, profile, replacement).unwrap();

        assert_eq!(
            read_profile_secret(tool, profile).unwrap().as_deref(),
            Some(replacement.as_slice())
        );

        restore_profile_secret(tool, profile, backup_id).unwrap();
        assert_eq!(
            read_profile_secret(tool, profile).unwrap().as_deref(),
            Some(original.as_slice())
        );

        delete_backup_secret(tool, profile, backup_id).unwrap();
        let backup_account = backup_account(tool, profile, backup_id);
        assert!(
            secure_backend::read_generic_password(BACKEND, SERVICE, Some(&backup_account))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    #[ignore = "opt-in real credential-store canary; set AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY=1"]
    fn real_credential_store_roundtrip_canary() {
        if std::env::var("AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY").as_deref() != Ok("1") {
            eprintln!(
                "skipping real credential-store canary: set AISW_ENABLE_REAL_CREDENTIAL_STORE_CANARY=1"
            );
            return;
        }

        let tool = Tool::Codex;
        let profile_name = canary_profile_name();
        let renamed_profile_name = format!("{profile_name}-renamed");
        let backup_id = format!("canary-backup-{}", std::process::id());
        let original = br#"{"token":"canary-original-token"}"#;
        let replacement = br#"{"token":"canary-replacement-token"}"#;

        let mut cleanup = CanaryCleanup::new();
        cleanup.track_profile(tool, &profile_name);
        cleanup.track_profile(tool, &renamed_profile_name);
        cleanup.track_backup(tool, &renamed_profile_name, &backup_id);

        write_profile_secret(tool, &profile_name, original).expect("write should succeed");
        let read_back = read_profile_secret(tool, &profile_name)
            .expect("read should succeed")
            .expect("secret should exist after write");
        assert_eq!(read_back, original);

        rename_profile_secret(tool, &profile_name, &renamed_profile_name)
            .expect("rename should succeed");
        assert!(
            read_profile_secret(tool, &profile_name)
                .expect("read old profile should succeed")
                .is_none(),
            "old profile secret should not exist after rename"
        );
        assert_eq!(
            read_profile_secret(tool, &renamed_profile_name)
                .expect("read renamed profile should succeed")
                .expect("renamed profile should exist"),
            original
        );

        snapshot_profile_secret(tool, &renamed_profile_name, &backup_id)
            .expect("snapshot should succeed");
        write_profile_secret(tool, &renamed_profile_name, replacement)
            .expect("overwrite should succeed");
        assert_eq!(
            read_profile_secret(tool, &renamed_profile_name)
                .expect("read replacement should succeed")
                .expect("replacement value should exist"),
            replacement
        );

        restore_profile_secret(tool, &renamed_profile_name, &backup_id)
            .expect("restore should succeed");
        assert_eq!(
            read_profile_secret(tool, &renamed_profile_name)
                .expect("read restored secret should succeed")
                .expect("restored secret should exist"),
            original
        );

        delete_backup_secret(tool, &renamed_profile_name, &backup_id)
            .expect("delete backup secret should succeed");
        delete_profile_secret(tool, &renamed_profile_name).expect("delete profile should succeed");
        assert!(
            read_profile_secret(tool, &renamed_profile_name)
                .expect("final read should succeed")
                .is_none(),
            "secret should not exist after delete"
        );
    }
}
