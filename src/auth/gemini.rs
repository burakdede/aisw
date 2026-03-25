use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::Utc;

use crate::config::{AuthMethod, ConfigStore, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::Tool;

const ENV_FILE: &str = ".env";
const KEY_VAR: &str = "GEMINI_API_KEY";

// Gemini CLI stores its OAuth token cache under $HOME/.gemini/.
// There is no documented GEMINI_HOME env var (as of 2026-03). The strategy:
// override HOME to a scratch dir so Gemini writes its cache there, then
// copy everything into the aisw profile dir. On switch, copy back to
// $HOME/.gemini/ (see auth::gemini::apply_token_cache).
const GEMINI_CACHE_DIR: &str = ".gemini";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(120);

pub fn add_api_key(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    key: &str,
    label: Option<String>,
) -> Result<()> {
    validate_api_key(key)?;

    profile_store.create(Tool::Gemini, name)?;

    let env_contents = format!("{}={}\n", KEY_VAR, key);
    profile_store
        .write_file(Tool::Gemini, name, ENV_FILE, env_contents.as_bytes())
        .inspect_err(|_| {
            let _ = profile_store.delete(Tool::Gemini, name);
        })?;

    config_store.add_profile(
        Tool::Gemini,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::ApiKey,
            label,
        },
    )?;

    Ok(())
}

pub fn validate_api_key(key: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!("Gemini API key must not be empty");
    }
    Ok(())
}

/// Read the stored API key from a profile's .env file.
pub fn read_api_key(profile_store: &ProfileStore, name: &str) -> Result<String> {
    let bytes = profile_store.read_file(Tool::Gemini, name, ENV_FILE)?;
    let contents = std::str::from_utf8(&bytes)
        .map_err(|e| anyhow::anyhow!("could not read .env file: {}", e))?;
    for line in contents.lines() {
        if let Some(val) = line.strip_prefix(&format!("{}=", KEY_VAR)) {
            return Ok(val.to_owned());
        }
    }
    anyhow::bail!(".env file missing '{}' entry", KEY_VAR)
}

/// Apply a profile's .env file to `dest` (typically `~/.gemini/.env`).
pub fn apply_env_file(
    profile_store: &ProfileStore,
    name: &str,
    dest: &std::path::Path,
) -> Result<()> {
    let bytes = profile_store.read_file(Tool::Gemini, name, ENV_FILE)?;
    std::fs::write(dest, &bytes)
        .map_err(|e| anyhow::anyhow!("could not write {}: {}", dest.display(), e))?;
    set_permissions_600(dest)
}

/// Start the Gemini OAuth flow using the installed `gemini` binary.
///
/// Overrides `HOME` so Gemini writes its token cache to a scratch directory
/// we control. After the process exits (or times out), copies all files from
/// `<scratch>/.gemini/` into the aisw profile dir with 0600 permissions.
pub fn add_oauth(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    gemini_bin: &Path,
) -> Result<()> {
    add_oauth_with(
        profile_store,
        config_store,
        name,
        label,
        gemini_bin,
        OAUTH_TIMEOUT,
    )
}

fn add_oauth_with(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    gemini_bin: &Path,
    timeout: Duration,
) -> Result<()> {
    let profile_dir = profile_store.create(Tool::Gemini, name)?;

    let result = run_oauth_flow(gemini_bin, &profile_dir, timeout).inspect_err(|_| {
        let _ = profile_store.delete(Tool::Gemini, name);
    })?;

    if result == 0 {
        let _ = profile_store.delete(Tool::Gemini, name);
        bail!(
            "Gemini login completed but no credential files were found in the token cache. \
             The OAuth flow may have failed silently."
        );
    }

    config_store.add_profile(
        Tool::Gemini,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            label,
        },
    )?;

    Ok(())
}

/// Spawn `gemini` with an overridden HOME, wait for it to exit, then copy
/// the resulting `$scratch/.gemini/` files into `profile_dir`.
/// Returns the number of files captured.
fn run_oauth_flow(gemini_bin: &Path, profile_dir: &Path, timeout: Duration) -> Result<usize> {
    let scratch = create_scratch_dir()?;

    let result = (|| {
        let mut child = Command::new(gemini_bin)
            .env("HOME", &scratch)
            .spawn()
            .with_context(|| format!("could not spawn {}", gemini_bin.display()))?;

        let status = wait_with_timeout(&mut child, timeout)?;
        if !status.success() {
            bail!(
                "gemini exited with status {}. Check for errors above.",
                status
            );
        }

        let cache_dir = scratch.join(GEMINI_CACHE_DIR);
        capture_cache_into_profile(&cache_dir, profile_dir)
    })();

    let _ = std::fs::remove_dir_all(&scratch);
    result
}

fn create_scratch_dir() -> Result<std::path::PathBuf> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let dir = std::env::temp_dir().join(format!("aisw-gemini-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("could not create scratch dir {}", dir.display()))?;
    Ok(dir)
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus> {
    use std::time::Instant;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().context("could not poll child process")? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Gemini login timed out after {}s.", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Copy every file from `cache_dir` into `profile_dir`, enforcing 0600.
/// Returns count of files copied.
fn capture_cache_into_profile(cache_dir: &Path, profile_dir: &Path) -> Result<usize> {
    if !cache_dir.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in std::fs::read_dir(cache_dir)
        .with_context(|| format!("could not read {}", cache_dir.display()))?
    {
        let entry = entry?;
        let src = entry.path();
        if src.is_symlink() || !src.is_file() {
            continue;
        }
        let filename = entry.file_name();
        let dst = profile_dir.join(&filename);
        std::fs::copy(&src, &dst)
            .with_context(|| format!("could not copy {} to {}", src.display(), dst.display()))?;
        set_permissions_600(&dst)?;
        count += 1;
    }
    Ok(count)
}

/// Copy token cache files from a profile dir back to `~/.gemini/` (the active location).
pub fn apply_token_cache(
    profile_store: &ProfileStore,
    name: &str,
    gemini_dir: &Path,
) -> Result<()> {
    std::fs::create_dir_all(gemini_dir)
        .with_context(|| format!("could not create {}", gemini_dir.display()))?;

    let profile_dir = profile_store.profile_dir(Tool::Gemini, name);
    for entry in std::fs::read_dir(&profile_dir)
        .with_context(|| format!("could not read {}", profile_dir.display()))?
    {
        let entry = entry?;
        let src = entry.path();
        if src.is_symlink() || !src.is_file() {
            continue;
        }
        // Skip the .env file — that's for API key profiles.
        if entry.file_name() == std::ffi::OsStr::new(ENV_FILE) {
            continue;
        }
        let dst = gemini_dir.join(entry.file_name());
        std::fs::copy(&src, &dst)
            .with_context(|| format!("could not copy {} to {}", src.display(), dst.display()))?;
        set_permissions_600(&dst)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_permissions_600(path: &std::path::Path) -> Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| anyhow::anyhow!("could not set permissions on {}: {}", path.display(), e))
}

#[cfg(not(unix))]
fn set_permissions_600(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;

    fn valid_key() -> &'static str {
        "AIzaSyTest1234567890"
    }

    fn stores(dir: &std::path::Path) -> (ProfileStore, ConfigStore) {
        (ProfileStore::new(dir), ConfigStore::new(dir))
    }

    #[test]
    fn validate_accepts_nonempty_key() {
        assert!(validate_api_key(valid_key()).is_ok());
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_api_key("").is_err());
        assert!(validate_api_key("  ").is_err());
    }

    #[test]
    fn add_creates_env_file() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();

        assert!(ps
            .profile_dir(Tool::Gemini, "default")
            .join(ENV_FILE)
            .exists());
    }

    #[test]
    fn env_file_has_correct_format() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();

        let contents = ps.read_file(Tool::Gemini, "default", ENV_FILE).unwrap();
        let text = std::str::from_utf8(&contents).unwrap();
        assert_eq!(text, format!("GEMINI_API_KEY={}\n", valid_key()));
    }

    #[test]
    fn read_api_key_roundtrip() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();
        assert_eq!(read_api_key(&ps, "default").unwrap(), valid_key());
    }

    #[test]
    fn apply_env_file_writes_to_dest() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();

        let dest_dir = tempdir().unwrap();
        let dest = dest_dir.path().join(".env");
        apply_env_file(&ps, "default", &dest).unwrap();

        let written = std::fs::read_to_string(&dest).unwrap();
        assert!(written.contains(valid_key()));
    }

    #[test]
    fn add_registers_in_config() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), Some("AI Studio".into())).unwrap();

        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles.gemini["default"].auth_method,
            AuthMethod::ApiKey
        );
        assert_eq!(
            config.profiles.gemini["default"].label.as_deref(),
            Some("AI Studio")
        );
    }

    #[test]
    #[cfg(unix)]
    fn env_file_has_600_permissions() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();

        let mode = fs::metadata(ps.profile_dir(Tool::Gemini, "default").join(ENV_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn duplicate_profile_errors() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", valid_key(), None).unwrap();
        let err = add_api_key(&ps, &cs, "default", valid_key(), None).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn invalid_key_does_not_create_profile() {
        let dir = tempdir().unwrap();
        let (ps, cs) = stores(dir.path());
        add_api_key(&ps, &cs, "default", "", None).unwrap_err();
        assert!(!ps.exists(Tool::Gemini, "default"));
    }

    // ---- OAuth tests ----

    #[cfg(unix)]
    fn make_oauth_mock(dir: &std::path::Path, write_creds: bool, exit_ok: bool) -> PathBuf {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let bin = dir.join("gemini");
        let body = match (write_creds, exit_ok) {
            (true, _) => "mkdir -p \"$HOME/.gemini\"\necho '{\"token\":\"tok\"}' > \"$HOME/.gemini/oauth_creds.json\"\nexit 0\n",
            (false, true) => "exit 0\n",
            (false, false) => "sleep 60\n",
        };
        fs::write(&bin, format!("#!/bin/sh\n{}", body)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_captures_credentials() {
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, true);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(&ps, &cs, "default", None, &bin, Duration::from_secs(10)).unwrap();

        assert!(ps.exists(Tool::Gemini, "default"));
        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles.gemini["default"].auth_method,
            AuthMethod::OAuth
        );
        assert!(ps
            .profile_dir(Tool::Gemini, "default")
            .join("oauth_creds.json")
            .exists());
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_errors_when_no_files_written() {
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, false, true);

        let (ps, cs) = stores(dir.path());
        let err =
            add_oauth_with(&ps, &cs, "default", None, &bin, Duration::from_secs(5)).unwrap_err();

        assert!(err.to_string().contains("no credential files"));
        assert!(!ps.exists(Tool::Gemini, "default"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_times_out_and_cleans_up() {
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, false, false);

        let (ps, cs) = stores(dir.path());
        let err =
            add_oauth_with(&ps, &cs, "default", None, &bin, Duration::from_secs(1)).unwrap_err();

        assert!(err.to_string().contains("timed out"));
        assert!(!ps.exists(Tool::Gemini, "default"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_credentials_have_600_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, true);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(&ps, &cs, "default", None, &bin, Duration::from_secs(10)).unwrap();

        let creds = ps
            .profile_dir(Tool::Gemini, "default")
            .join("oauth_creds.json");
        let mode = std::fs::metadata(&creds).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    #[cfg(unix)]
    fn apply_token_cache_copies_non_env_files() {
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, true);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(&ps, &cs, "default", None, &bin, Duration::from_secs(10)).unwrap();

        let dest_dir = dir.path().join("fake_gemini_home");
        std::fs::create_dir_all(&dest_dir).unwrap();
        apply_token_cache(&ps, "default", &dest_dir).unwrap();

        assert!(dest_dir.join("oauth_creds.json").exists());
    }
}
