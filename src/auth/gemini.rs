use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::Utc;

use super::files;
use super::identity;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend, ProfileMeta};
use crate::live_apply::LiveFileChange;
use crate::profile::ProfileStore;
use crate::terminal::TerminalGuard;
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
const OAUTH_PRIMARY_FILES: &[&str] = &["oauth_creds.json"];
const POST_AUTH_EXIT_GRACE: Duration = Duration::from_secs(2);

pub fn live_dir(user_home: &Path) -> PathBuf {
    user_home.join(GEMINI_CACHE_DIR)
}

#[derive(Debug, Clone)]
pub struct LiveImportSelection {
    pub method: AuthMethod,
    pub source_description: String,
    pub env_file: PathBuf,
    pub oauth_files: Vec<files::RegularFile>,
    pub has_both_sources: bool,
}

pub fn detect_live_import_selection(user_home: &Path) -> Result<Option<LiveImportSelection>> {
    let gemini_dir = live_dir(user_home);
    let env_file = gemini_dir.join(ENV_FILE);
    let oauth_files = live_oauth_files_for_import(user_home)?;
    let has_env = env_file.exists();
    let has_oauth = !oauth_files.is_empty();

    if !has_env && !has_oauth {
        return Ok(None);
    }

    let (method, source_description) = if has_env {
        (AuthMethod::ApiKey, format!("found {}", env_file.display()))
    } else {
        let primary_file = preferred_live_oauth_file(&oauth_files)
            .context("could not determine primary Gemini OAuth credential file")?;
        (
            AuthMethod::OAuth,
            live_import_source_description(&primary_file.path, oauth_files.len()),
        )
    };

    Ok(Some(LiveImportSelection {
        method,
        source_description,
        env_file,
        oauth_files,
        has_both_sources: has_env && has_oauth,
    }))
}

pub fn live_oauth_files_for_import(user_home: &Path) -> Result<Vec<files::RegularFile>> {
    let gemini_dir = live_dir(user_home);
    if !gemini_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = files::list_regular_files(&gemini_dir)?
        .into_iter()
        .filter(|file| file.file_name != OsStr::new(ENV_FILE))
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(files)
}

pub fn preferred_live_oauth_file(files: &[files::RegularFile]) -> Option<&files::RegularFile> {
    files
        .iter()
        .find(|file| file.file_name == OsStr::new("settings.json"))
        .or_else(|| {
            files
                .iter()
                .find(|file| file.file_name == OsStr::new("oauth_creds.json"))
        })
        .or_else(|| files.first())
}

pub fn live_import_source_description(path: &Path, total_files: usize) -> String {
    if total_files > 1 {
        format!("found {} (+{} more files)", path.display(), total_files - 1)
    } else {
        format!("found {}", path.display())
    }
}

pub fn existing_oauth_profile_for_live_files(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    files: &[files::RegularFile],
) -> Result<Option<String>> {
    for file in files {
        let bytes = std::fs::read(&file.path)
            .with_context(|| format!("could not read {}", file.path.display()))?;
        if let Some(existing_name) = identity::existing_oauth_profile_for_json_bytes(
            profile_store,
            config_store,
            Tool::Gemini,
            &bytes,
        )? {
            return Ok(Some(existing_name));
        }
    }

    Ok(None)
}

pub fn copy_live_oauth_files_into_profile(
    profile_store: &ProfileStore,
    profile_name: &str,
    files: &[files::RegularFile],
) -> Result<()> {
    for file in files {
        let file_name = file.file_name.to_string_lossy().into_owned();
        profile_store.copy_file_into(Tool::Gemini, profile_name, &file.path, &file_name)?;
    }

    Ok(())
}

pub fn add_api_key(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    key: &str,
    label: Option<String>,
) -> Result<()> {
    validate_api_key(key)?;

    if let Some(existing_name) = identity::existing_api_key_profile_for_secret(
        profile_store,
        config_store,
        Tool::Gemini,
        key,
    )? {
        bail!(
            "Gemini API key already exists as profile '{}'.\n  \
             Use that profile or provide a different API key.",
            existing_name
        );
    }

    profile_store.create(Tool::Gemini, name)?;

    let env_contents = format!("{}={}\n", KEY_VAR, key);
    files::cleanup_profile_on_error(
        profile_store.write_file(Tool::Gemini, name, ENV_FILE, env_contents.as_bytes()),
        profile_store,
        Tool::Gemini,
        name,
    )?;

    config_store.add_profile(
        Tool::Gemini,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::ApiKey,
            credential_backend: CredentialBackend::File,
            label,
        },
    )?;

    Ok(())
}

pub fn validate_api_key(key: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!(
            "Gemini API key must not be empty.\n  \
             Get your API key at aistudio.google.com → Get API Key."
        );
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
    anyhow::bail!(
        ".env file for profile '{}' is missing the '{}' entry.\n  \
         Run 'aisw remove gemini {}' then 'aisw add gemini {}' to reconfigure.",
        name,
        KEY_VAR,
        name,
        name
    )
}

/// Apply a profile's .env file to `dest` (typically `~/.gemini/.env`).
pub fn apply_env_file(
    profile_store: &ProfileStore,
    name: &str,
    dest: &std::path::Path,
) -> Result<()> {
    files::apply_profile_file(
        profile_store,
        Tool::Gemini,
        name,
        ENV_FILE,
        dest.to_path_buf(),
    )
}

pub fn live_env_matches(profile_store: &ProfileStore, name: &str, dest: &Path) -> Result<bool> {
    files::stored_profile_file_matches_live(profile_store, Tool::Gemini, name, ENV_FILE, dest)
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
        Duration::from_millis(500),
    )
}

#[cfg(any(test, debug_assertions))]
pub fn add_oauth_with_for_test(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    gemini_bin: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<()> {
    add_oauth_with(
        profile_store,
        config_store,
        name,
        label,
        gemini_bin,
        timeout,
        poll_interval,
    )
}

fn add_oauth_with(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    name: &str,
    label: Option<String>,
    gemini_bin: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<()> {
    let profile_dir = profile_store.create(Tool::Gemini, name)?;

    let result = files::cleanup_profile_on_error(
        run_oauth_flow(gemini_bin, &profile_dir, timeout, poll_interval),
        profile_store,
        Tool::Gemini,
        name,
    )?;

    if result == 0 {
        files::cleanup_profile(profile_store, Tool::Gemini, name);
        bail!(
            "Gemini login completed but no credential files were found in the token cache.\n  \
             The OAuth flow may have failed silently. Try running 'aisw add gemini {}' again,\n  \
             or use an API key instead: 'aisw add gemini {} --api-key <key>'.",
            name,
            name
        );
    }

    files::cleanup_profile_on_error(
        identity::ensure_unique_oauth_identity(
            profile_store,
            config_store,
            Tool::Gemini,
            name,
            CredentialBackend::File,
        ),
        profile_store,
        Tool::Gemini,
        name,
    )?;

    config_store.add_profile(
        Tool::Gemini,
        name,
        ProfileMeta {
            added_at: Utc::now(),
            auth_method: AuthMethod::OAuth,
            credential_backend: CredentialBackend::File,
            label,
        },
    )?;

    // Best-effort identity display after capture
    match extract_captured_identity(profile_store, name) {
        Some(email) => crate::output::print_kv("Captured account", &email),
        None => crate::output::print_info("(could not verify captured identity)"),
    }

    Ok(())
}

/// Decode `oauth_creds.json` from the profile dir and extract the `email` from the `id_token` JWT.
/// Returns None if file missing, malformed, or JWT has no email claim.
pub fn extract_captured_identity(profile_store: &ProfileStore, name: &str) -> Option<String> {
    let bytes = profile_store
        .read_file(Tool::Gemini, name, "oauth_creds.json")
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let id_token = v.get("id_token").and_then(|t| t.as_str())?;
    decode_jwt_email_from_token(id_token)
}

fn decode_jwt_email_from_token(jwt: &str) -> Option<String> {
    let payload = crate::util::jwt::decode_jwt_payload(jwt)?;
    payload
        .get("email")
        .and_then(|e| e.as_str())
        .map(String::from)
}

/// Spawn `gemini` with an overridden HOME, wait for it to exit, then copy
/// the resulting `$scratch/.gemini/` files into `profile_dir`.
/// Returns the number of files captured.
fn run_oauth_flow(
    gemini_bin: &Path,
    profile_dir: &Path,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<usize> {
    let scratch = create_scratch_dir()?;
    let scratch_workdir = scratch.join("workspace");
    std::fs::create_dir(&scratch_workdir).with_context(|| {
        format!(
            "could not create Gemini scratch workspace {}",
            scratch_workdir.display()
        )
    })?;
    prepare_scratch_home(&scratch, &scratch_workdir)?;

    crate::output::print_info("Steps:");
    crate::output::print_info("  1. Complete Gemini sign-in in your browser");
    crate::output::print_info("  2. Stay in this terminal; aisw will detect completion");
    let spinner = crate::output::start_spinner("Waiting for Gemini OAuth...");

    let result = (|| {
        let terminal = TerminalGuard::capture();
        let mut child = spawn_oauth_child(gemini_bin, &scratch, &scratch_workdir)?;

        let cache_dir = scratch.join(GEMINI_CACHE_DIR);
        let deadline = std::time::Instant::now() + timeout;

        loop {
            let captured = capture_oauth_cache_into_profile(&cache_dir, profile_dir)?;
            if captured > 0 {
                terminal.restore();
                finalize_child_after_success(&mut child, POST_AUTH_EXIT_GRACE);
                return Ok(captured);
            }

            if let Some(status) = child.try_wait().context("could not poll child process")? {
                terminal.restore();
                if !status.success() {
                    bail!(
                        "gemini exited with status {}. Check for errors above.",
                        status
                    );
                }
                return capture_oauth_cache_into_profile(&cache_dir, profile_dir);
            }

            if std::time::Instant::now() >= deadline {
                terminal.restore();
                stop_interactive_child(&mut child);
                bail!("Gemini login timed out after {}s.", timeout.as_secs());
            }

            std::thread::sleep(poll_interval);
        }
    })();

    match &result {
        Ok(_) => {
            if let Some(ref sp) = spinner {
                sp.finish_with_message("\u{2713} Credentials captured");
            }
        }
        Err(_) => {
            if let Some(ref sp) = spinner {
                sp.finish_and_clear();
            }
        }
    }

    // Always clean up the scratch HOME regardless of outcome. On success the
    // credential files have already been copied into the profile directory.
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

fn spawn_oauth_child(
    gemini_bin: &Path,
    scratch: &Path,
    scratch_workdir: &Path,
) -> Result<std::process::Child> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::process::CommandExt;

        let child = Command::new("/usr/bin/script")
            .arg("-q")
            .arg("/dev/null")
            .arg(gemini_bin)
            .env("HOME", scratch)
            .current_dir(scratch_workdir)
            // Create a dedicated process group so we can terminate the entire
            // OAuth process tree (script + gemini child) reliably.
            .process_group(0)
            .spawn()
            .with_context(|| format!("could not spawn {}", gemini_bin.display()))?;
        Ok(child)
    }

    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(unix)]
        use std::os::unix::process::CommandExt;

        let mut cmd = Command::new(gemini_bin);
        cmd.env("HOME", scratch).current_dir(scratch_workdir);
        #[cfg(unix)]
        cmd.process_group(0);
        let child = cmd
            .spawn()
            .with_context(|| format!("could not spawn {}", gemini_bin.display()))?;
        Ok(child)
    }
}

fn stop_interactive_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        send_signal_to_oauth_tree(pid, libc::SIGINT);
        for _ in 0..10 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        // Best effort: terminate direct descendants before escalating parent.
        #[cfg(target_os = "linux")]
        kill_child_descendants(pid, libc::SIGTERM);

        send_signal_to_oauth_tree(pid, libc::SIGTERM);
        for _ in 0..10 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        #[cfg(target_os = "linux")]
        kill_child_descendants(pid, libc::SIGKILL);
    }

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn send_signal_to_oauth_tree(pid: i32, signal: i32) {
    // Try process-group signaling first (negative PID), then direct PID.
    let _ = unsafe { libc::kill(-pid, signal) };
    let _ = unsafe { libc::kill(pid, signal) };
}

#[cfg(target_os = "linux")]
fn kill_child_descendants(root_pid: i32, signal: i32) {
    fn read_children(pid: i32) -> Vec<i32> {
        let path = format!("/proc/{}/task/{}/children", pid, pid);
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        contents
            .split_whitespace()
            .filter_map(|s| s.parse::<i32>().ok())
            .collect()
    }

    fn walk(pid: i32, out: &mut Vec<i32>) {
        for child in read_children(pid) {
            out.push(child);
            walk(child, out);
        }
    }

    let mut descendants = Vec::new();
    walk(root_pid, &mut descendants);
    for pid in descendants.into_iter().rev() {
        let _ = unsafe { libc::kill(pid, signal) };
    }
}

fn finalize_child_after_success(child: &mut std::process::Child, grace: Duration) {
    let deadline = std::time::Instant::now() + grace;
    while std::time::Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    stop_interactive_child(child);
}

fn prepare_scratch_home(scratch: &Path, scratch_workdir: &Path) -> Result<()> {
    let gemini_dir = scratch.join(GEMINI_CACHE_DIR);
    std::fs::create_dir(&gemini_dir)
        .with_context(|| format!("could not create {}", gemini_dir.display()))?;
    let trusted_folders = serde_json::json!({
        scratch_workdir.display().to_string(): "TRUST_FOLDER"
    });
    std::fs::write(
        gemini_dir.join("trustedFolders.json"),
        serde_json::to_vec_pretty(&trusted_folders)
            .context("could not serialize Gemini trusted folders")?,
    )
    .with_context(|| {
        format!(
            "could not write {}",
            gemini_dir.join("trustedFolders.json").display()
        )
    })?;
    Ok(())
}

fn create_scratch_dir() -> Result<std::path::PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    // Use an atomic counter for uniqueness within one process and a time-based
    // suffix to avoid collisions with stale directories from previous runs.
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let temp_root = std::env::temp_dir();
    let pid = std::process::id();

    for _ in 0..32 {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = temp_root.join(format!("aisw-gemini-{}-{}-{}", pid, id, nanos));

        match std::fs::create_dir(&dir) {
            Ok(_) => return Ok(dir),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("could not create scratch dir {}", dir.display()))
            }
        }
    }

    bail!("could not create a unique Gemini scratch directory after multiple attempts")
}

/// Copy every file from `cache_dir` into `profile_dir`, enforcing 0600.
/// Returns count of files copied.
fn capture_oauth_cache_into_profile(cache_dir: &Path, profile_dir: &Path) -> Result<usize> {
    if !cache_dir.exists() {
        return Ok(0);
    }
    if !has_oauth_credentials(cache_dir)? {
        return Ok(0);
    }
    let mut count = 0;
    for file in files::list_regular_files(cache_dir)? {
        let dst = profile_dir.join(&file.file_name);
        std::fs::copy(&file.path, &dst).with_context(|| {
            format!(
                "could not copy {} to {}",
                file.path.display(),
                dst.display()
            )
        })?;
        files::set_permissions_600(&dst)?;
        count += 1;
    }
    Ok(count)
}

fn has_oauth_credentials(cache_dir: &Path) -> Result<bool> {
    for file in files::list_regular_files(cache_dir)? {
        let name = file.file_name.to_string_lossy();
        if OAUTH_PRIMARY_FILES
            .iter()
            .any(|candidate| *candidate == name)
        {
            return Ok(true);
        }
    }
    Ok(false)
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
    let mut changes = Vec::new();
    for file in files::list_regular_files(&profile_dir)? {
        // Skip the .env file — that's for API key profiles.
        if file.file_name == std::ffi::OsStr::new(ENV_FILE) {
            continue;
        }
        let dst = gemini_dir.join(&file.file_name);
        let contents = std::fs::read(&file.path)
            .with_context(|| format!("could not read {}", file.path.display()))?;
        changes.push(LiveFileChange::write(dst, contents));
    }

    let env_file = gemini_dir.join(ENV_FILE);
    changes.push(LiveFileChange::delete(env_file));

    crate::live_apply::apply_transaction(changes)
}

pub fn live_token_cache_matches(
    profile_store: &ProfileStore,
    name: &str,
    gemini_dir: &Path,
) -> Result<bool> {
    let profile_dir = profile_store.profile_dir(Tool::Gemini, name);
    if !gemini_dir.exists() {
        return Ok(false);
    }
    if gemini_dir.join(ENV_FILE).exists() {
        return Ok(false);
    }

    let mut saw_file = false;
    for file in files::list_regular_files(&profile_dir)? {
        saw_file = true;
        let live = gemini_dir.join(&file.file_name);
        if !live.exists() {
            return Ok(false);
        }
        let src_bytes = std::fs::read(&file.path)
            .with_context(|| format!("could not read {}", file.path.display()))?;
        let live_bytes =
            std::fs::read(&live).with_context(|| format!("could not read {}", live.display()))?;
        if src_bytes != live_bytes {
            return Ok(false);
        }
    }

    Ok(saw_file)
}

#[cfg(test)]
mod tests {
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
    fn validate_empty_key_error_mentions_aistudio() {
        let err = validate_api_key("").unwrap_err();
        assert!(err.to_string().contains("aistudio.google.com"));
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
            config.profiles_for(Tool::Gemini)["default"].auth_method,
            AuthMethod::ApiKey
        );
        assert_eq!(
            config.profiles_for(Tool::Gemini)["default"]
                .label
                .as_deref(),
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

    // Poll interval used in all OAuth tests.
    const TEST_POLL: Duration = Duration::from_millis(10);

    /// Creates a mock gemini binary.
    ///
    /// `write_creds=true, long_running=false`  → writes oauth_creds.json and exits 0
    /// `write_creds=true, long_running=true`   → writes oauth_creds.json and keeps running
    /// `write_creds=false, long_running=false` → exits 0 immediately, no files
    /// `write_creds=false, long_running=true`  → sleeps briefly so timeout can fire.
    #[cfg(unix)]
    fn make_oauth_mock(dir: &std::path::Path, write_creds: bool, long_running: bool) -> PathBuf {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let bin = dir.join("gemini");
        let body = match (write_creds, long_running) {
            (true, _) => {
                if long_running {
                    "mkdir -p \"$HOME/.gemini\"\n\
                     echo '{\"token\":\"tok\"}' > \"$HOME/.gemini/oauth_creds.json\"\n\
                     sleep 5\n"
                } else {
                    "mkdir -p \"$HOME/.gemini\"\n\
                     echo '{\"token\":\"tok\"}' > \"$HOME/.gemini/oauth_creds.json\"\n\
                     exit 0\n"
                }
            }
            (false, false) => "exit 0\n",
            // Sleep briefly — long enough to outlast a 200ms test timeout, short
            // enough that any orphaned `sleep` process cleans itself up quickly.
            (false, true) => "sleep 0.5\n",
        };
        fs::write(&bin, format!("#!/bin/sh\n{}", body)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_captures_credentials() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, false);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(ps.exists(Tool::Gemini, "default"));
        let config = cs.load().unwrap();
        assert_eq!(
            config.profiles_for(Tool::Gemini)["default"].auth_method,
            AuthMethod::OAuth
        );
        assert!(ps
            .profile_dir(Tool::Gemini, "default")
            .join("oauth_creds.json")
            .exists());
    }

    #[test]
    #[cfg(unix)]
    fn oauth_scratch_dir_cleaned_up_on_success() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, false);

        let tmp_before: std::collections::HashSet<_> = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("aisw-gemini-"))
                    .unwrap_or(false)
            })
            .collect();

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let tmp_after: std::collections::HashSet<_> = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("aisw-gemini-"))
                    .unwrap_or(false)
            })
            .collect();

        let leaked: Vec<_> = tmp_after.difference(&tmp_before).collect();
        assert!(
            leaked.is_empty(),
            "scratch dirs leaked after successful OAuth: {leaked:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_captures_credentials_without_waiting_for_gemini_shell_exit() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, true);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(ps
            .profile_dir(Tool::Gemini, "default")
            .join("oauth_creds.json")
            .exists());
    }

    #[test]
    #[cfg(all(unix, target_os = "linux"))]
    fn oauth_flow_stops_interactive_child_after_success() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let signal_file = dir.path().join("received-signal");
        let signal_file_escaped = signal_file.display().to_string().replace('\'', "'\"'\"'");

        let bin = bin_dir.join("gemini");
        fs::write(
            &bin,
            format!(
                "#!/bin/sh\n\
                 trap 'echo stopped > '{}' ; exit 0' INT TERM\n\
                 mkdir -p \"$HOME/.gemini\"\n\
                 echo '{{\"token\":\"tok\"}}' > \"$HOME/.gemini/oauth_creds.json\"\n\
                 while :; do sleep 1; done\n",
                signal_file_escaped
            ),
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        assert!(ps
            .profile_dir(Tool::Gemini, "default")
            .join("oauth_creds.json")
            .exists());
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while std::time::Instant::now() < deadline && !signal_file.exists() {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            signal_file.exists(),
            "expected OAuth child to receive shutdown signal after capture"
        );
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_ignores_non_auth_startup_files() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("gemini");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             mkdir -p \"$HOME/.gemini\"\n\
             echo '{\"theme\":\"dark\"}' > \"$HOME/.gemini/settings.json\"\n\
             sleep 0.5\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        let err = add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_millis(200),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(err.to_string().contains("timed out"));
        assert!(!ps.exists(Tool::Gemini, "default"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_runs_gemini_in_isolated_workspace() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("gemini");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             [ \"$(basename \"$PWD\")\" = \"workspace\" ] || exit 7\n\
             mkdir -p \"$HOME/.gemini\"\n\
             echo '{\"token\":\"tok\"}' > \"$HOME/.gemini/oauth_creds.json\"\n",
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();
        assert!(ps
            .profile_dir(Tool::Gemini, "default")
            .join("oauth_creds.json")
            .exists());
    }

    #[test]
    #[cfg(not(windows))]
    fn prepare_scratch_home_pretrusts_workspace() {
        let dir = tempdir().unwrap();
        let scratch = dir.path().join("scratch");
        let workspace = scratch.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        prepare_scratch_home(&scratch, &workspace).unwrap();

        let trusted =
            std::fs::read_to_string(scratch.join(".gemini").join("trustedFolders.json")).unwrap();
        assert!(trusted.contains(&workspace.display().to_string()));
        assert!(trusted.contains("TRUST_FOLDER"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_errors_when_no_files_written() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, false, false);

        let (ps, cs) = stores(dir.path());
        let err = add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(err.to_string().contains("no credential files"));
        assert!(!ps.exists(Tool::Gemini, "default"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_flow_times_out_and_cleans_up() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        // Mock sleeps 0.5s — longer than the 200ms test timeout so the timeout
        // fires while the mock is still running.  The orphaned `sleep 0.5`
        // subprocess exits on its own within half a second.
        let bin = make_oauth_mock(&bin_dir, false, true);

        let (ps, cs) = stores(dir.path());
        let err = add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_millis(200),
            TEST_POLL,
        )
        .unwrap_err();

        assert!(err.to_string().contains("timed out"));
        assert!(!ps.exists(Tool::Gemini, "default"));
    }

    #[test]
    #[cfg(unix)]
    fn oauth_credentials_have_600_permissions() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, false);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let creds = ps
            .profile_dir(Tool::Gemini, "default")
            .join("oauth_creds.json");
        let mode = std::fs::metadata(&creds).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    #[cfg(unix)]
    fn apply_token_cache_copies_non_env_files() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, false);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let dest_dir = dir.path().join("fake_gemini_home");
        std::fs::create_dir_all(&dest_dir).unwrap();
        apply_token_cache(&ps, "default", &dest_dir).unwrap();

        assert!(dest_dir.join("oauth_creds.json").exists());
    }

    #[test]
    #[cfg(unix)]
    fn apply_token_cache_removes_stale_env_file() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, false);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let dest_dir = dir.path().join("fake_gemini_home");
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join(ENV_FILE), b"GEMINI_API_KEY=stale\n").unwrap();

        apply_token_cache(&ps, "default", &dest_dir).unwrap();

        assert!(!dest_dir.join(ENV_FILE).exists());
        assert!(dest_dir.join("oauth_creds.json").exists());
    }

    #[test]
    fn live_oauth_files_for_import_skips_env_and_sorts() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let gemini_dir = user_home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(gemini_dir.join(".env"), "GEMINI_API_KEY=test\n").unwrap();
        std::fs::write(gemini_dir.join("z.json"), "{}").unwrap();
        std::fs::write(gemini_dir.join("oauth_creds.json"), "{}").unwrap();

        let files = live_oauth_files_for_import(&user_home).unwrap();
        let names = files
            .iter()
            .map(|file| file.file_name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["oauth_creds.json", "z.json"]);
    }

    #[test]
    fn preferred_live_oauth_file_prefers_settings_json() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let gemini_dir = user_home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(gemini_dir.join("oauth_creds.json"), "{}").unwrap();
        std::fs::write(gemini_dir.join("settings.json"), "{}").unwrap();

        let files = live_oauth_files_for_import(&user_home).unwrap();
        let preferred = preferred_live_oauth_file(&files).expect("preferred file");

        assert_eq!(preferred.file_name, "settings.json");
    }

    #[test]
    fn preferred_live_oauth_file_prefers_oauth_creds_when_settings_missing() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let gemini_dir = user_home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(gemini_dir.join("a.json"), "{}").unwrap();
        std::fs::write(gemini_dir.join("oauth_creds.json"), "{}").unwrap();

        let files = live_oauth_files_for_import(&user_home).unwrap();
        let preferred = preferred_live_oauth_file(&files).expect("preferred file");
        assert_eq!(preferred.file_name, "oauth_creds.json");
    }

    #[test]
    fn preferred_live_oauth_file_falls_back_to_first_file() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let gemini_dir = user_home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(gemini_dir.join("b.json"), "{}").unwrap();
        std::fs::write(gemini_dir.join("a.json"), "{}").unwrap();

        let files = live_oauth_files_for_import(&user_home).unwrap();
        let preferred = preferred_live_oauth_file(&files).expect("preferred file");
        // live_oauth_files_for_import sorts filenames ascending.
        assert_eq!(preferred.file_name, "a.json");
    }

    #[test]
    fn detect_live_import_selection_prefers_env_when_both_sources_exist() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let gemini_dir = user_home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(gemini_dir.join(".env"), "GEMINI_API_KEY=test\n").unwrap();
        std::fs::write(gemini_dir.join("oauth_creds.json"), "{}").unwrap();

        let selection = detect_live_import_selection(&user_home)
            .unwrap()
            .expect("selection should be present");
        assert_eq!(selection.method, AuthMethod::ApiKey);
        assert!(selection.has_both_sources);
        assert_eq!(selection.oauth_files.len(), 1);
    }

    #[test]
    fn detect_live_import_selection_uses_oauth_when_env_absent() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        let gemini_dir = user_home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(gemini_dir.join("oauth_creds.json"), "{}").unwrap();
        std::fs::write(gemini_dir.join("z.json"), "{}").unwrap();

        let selection = detect_live_import_selection(&user_home)
            .unwrap()
            .expect("selection should be present");
        assert_eq!(selection.method, AuthMethod::OAuth);
        assert!(!selection.has_both_sources);
        assert_eq!(selection.oauth_files.len(), 2);
        assert!(
            selection.source_description.contains("oauth_creds.json")
                && selection.source_description.contains("(+1 more files)"),
            "unexpected source description: {}",
            selection.source_description
        );
    }

    #[test]
    fn detect_live_import_selection_returns_none_when_empty() {
        let dir = tempdir().unwrap();
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(user_home.join(".gemini")).unwrap();

        let selection = detect_live_import_selection(&user_home).unwrap();
        assert!(selection.is_none());
    }

    #[test]
    #[cfg(unix)]
    fn live_token_cache_match_fails_when_stale_env_file_exists() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, false);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let dest_dir = dir.path().join("fake_gemini_home");
        std::fs::create_dir_all(&dest_dir).unwrap();
        apply_token_cache(&ps, "default", &dest_dir).unwrap();
        std::fs::write(dest_dir.join(ENV_FILE), b"GEMINI_API_KEY=stale\n").unwrap();

        assert!(!live_token_cache_matches(&ps, "default", &dest_dir).unwrap());
    }

    #[test]
    #[cfg(unix)]
    fn live_token_cache_matches_after_apply_token_cache() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = make_oauth_mock(&bin_dir, true, false);

        let (ps, cs) = stores(dir.path());
        add_oauth_with(
            &ps,
            &cs,
            "default",
            None,
            &bin,
            Duration::from_secs(2),
            TEST_POLL,
        )
        .unwrap();

        let dest_dir = dir.path().join("fake_gemini_home");
        std::fs::create_dir_all(&dest_dir).unwrap();
        apply_token_cache(&ps, "default", &dest_dir).unwrap();

        assert!(live_token_cache_matches(&ps, "default", &dest_dir).unwrap());
    }

    #[test]
    fn read_api_key_errors_when_env_missing_expected_key_var() {
        let dir = tempdir().unwrap();
        let ps = ProfileStore::new(dir.path());
        ps.create(Tool::Gemini, "default").unwrap();
        ps.write_file(Tool::Gemini, "default", ENV_FILE, b"OTHER_VAR=value\n")
            .unwrap();

        let err = read_api_key(&ps, "default").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("missing the 'GEMINI_API_KEY' entry")
                && msg.contains("aisw remove gemini default")
                && msg.contains("aisw add gemini default"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn live_import_source_description_formats_single_and_multiple() {
        let single = live_import_source_description(Path::new("/tmp/oauth_creds.json"), 1);
        let multiple = live_import_source_description(Path::new("/tmp/oauth_creds.json"), 3);
        assert_eq!(single, "found /tmp/oauth_creds.json");
        assert_eq!(multiple, "found /tmp/oauth_creds.json (+2 more files)");
    }

    // ---- extract_captured_identity tests ----

    fn make_fixture_jwt(email: &str) -> String {
        let payload_json = format!(r#"{{"email":"{}","exp":9999999999}}"#, email);
        let payload = crate::util::jwt::encode_jwt_payload_for_test(payload_json.as_bytes());
        format!("eyJhbGciOiJIUzI1NiJ9.{}.sig", payload)
    }

    #[test]
    fn extract_identity_with_valid_jwt() {
        let dir = tempdir().unwrap();
        let ps = ProfileStore::new(dir.path());
        ps.create(Tool::Gemini, "work").unwrap();
        let jwt = make_fixture_jwt("test@example.com");
        let creds = format!(r#"{{"id_token":"{}"}}"#, jwt);
        ps.write_file(Tool::Gemini, "work", "oauth_creds.json", creds.as_bytes())
            .unwrap();

        let identity = extract_captured_identity(&ps, "work");
        assert_eq!(identity.as_deref(), Some("test@example.com"));
    }

    #[test]
    fn extract_identity_with_malformed_id_token_returns_none() {
        let dir = tempdir().unwrap();
        let ps = ProfileStore::new(dir.path());
        ps.create(Tool::Gemini, "work").unwrap();
        ps.write_file(
            Tool::Gemini,
            "work",
            "oauth_creds.json",
            br#"{"id_token":"not.a.valid.jwt.at.all"}"#,
        )
        .unwrap();

        let identity = extract_captured_identity(&ps, "work");
        assert!(identity.is_none());
    }

    #[test]
    fn extract_identity_with_missing_id_token_returns_none() {
        let dir = tempdir().unwrap();
        let ps = ProfileStore::new(dir.path());
        ps.create(Tool::Gemini, "work").unwrap();
        ps.write_file(
            Tool::Gemini,
            "work",
            "oauth_creds.json",
            br#"{"token":"something"}"#,
        )
        .unwrap();

        let identity = extract_captured_identity(&ps, "work");
        assert!(identity.is_none());
    }
}
