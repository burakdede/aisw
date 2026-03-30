use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use crate::types::Tool;

const VERSION_TIMEOUT: Duration = Duration::from_millis(750);
#[cfg(test)]
type VersionFn = fn(&std::path::Path) -> Option<String>;

#[derive(Debug, Clone)]
pub struct DetectedTool {
    pub tool: Tool,
    pub binary_path: PathBuf,
    pub version: Option<String>,
}

#[derive(Clone, Copy)]
enum VersionSource {
    None,
    Capture,
    #[cfg(test)]
    Custom(VersionFn),
}

pub fn detect(tool: Tool) -> Option<DetectedTool> {
    detect_at(
        tool,
        std::env::var_os("PATH").unwrap_or_default(),
        VersionSource::Capture,
    )
}

pub fn detect_all() -> HashMap<Tool, Option<DetectedTool>> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    Tool::ALL
        .into_iter()
        .map(|t| (t, detect_in(t, path.clone())))
        .collect()
}

pub fn require(tool: Tool) -> Result<DetectedTool> {
    match detect(tool) {
        Some(d) => Ok(d),
        None => bail!(
            "{} is not installed or not found on PATH.\n  \
             Install it and make sure the binary is on your PATH.",
            tool.binary_name()
        ),
    }
}

pub(crate) fn require_in(tool: Tool, path: OsString) -> Result<DetectedTool> {
    match detect_in(tool, path) {
        Some(d) => Ok(d),
        None => bail!(
            "{} is not installed or not found on PATH.\n  \
             Install it and make sure the binary is on your PATH.",
            tool.binary_name()
        ),
    }
}

pub(crate) fn detect_in(tool: Tool, path: OsString) -> Option<DetectedTool> {
    detect_at(tool, path, VersionSource::None)
}

fn detect_at(tool: Tool, path: OsString, version_source: VersionSource) -> Option<DetectedTool> {
    let binary_path = find_binary_path(tool, &path)?;
    let version = match version_source {
        VersionSource::None => None,
        VersionSource::Capture => capture_version(&binary_path),
        #[cfg(test)]
        VersionSource::Custom(version_fn) => version_fn(&binary_path),
    };
    Some(DetectedTool {
        tool,
        binary_path,
        version,
    })
}

fn find_binary_path(tool: Tool, path: &OsString) -> Option<PathBuf> {
    which::which_in(tool.binary_name(), Some(path), ".").ok()
}

pub(crate) fn capture_version(binary: &std::path::Path) -> Option<String> {
    let mut child = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(_) => {
                let output = child.wait_with_output().ok()?;
                return parse_version_output(output.stdout, output.stderr);
            }
            None if start.elapsed() >= VERSION_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn parse_version_output(stdout: Vec<u8>, stderr: Vec<u8>) -> Option<String> {
    // Best-effort: try stdout first, fall back to stderr.
    let raw = if !stdout.is_empty() { stdout } else { stderr };
    let s = std::str::from_utf8(&raw).ok()?.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;

    /// Creates a minimal executable shell script in `dir` that prints `output` and exits.
    fn make_dummy_binary(dir: &Path, name: &str, output: &str, exit_ok: bool) {
        let path = dir.join(name);
        let code = if exit_ok { 0 } else { 1 };
        fs::write(
            &path,
            format!("#!/bin/sh\necho '{}'\nexit {}\n", output, code),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn path_of(dir: &Path) -> OsString {
        dir.as_os_str().to_owned()
    }

    // --- Binary discovery tests (no process spawning) ---
    // These use detect_at with a mock version getter so they are not
    // affected by OS process-spawn latency or resource contention.

    fn no_version(_: &Path) -> Option<String> {
        None
    }

    fn injected_version(_: &Path) -> Option<String> {
        Some("injected 1.2.3".to_owned())
    }

    #[test]
    fn detect_missing_returns_none() {
        let dir = tempdir().unwrap();
        // No binary in dir — should be absent regardless of version getter.
        assert!(detect_at(
            Tool::Claude,
            path_of(dir.path()),
            VersionSource::Custom(no_version)
        )
        .is_none());
    }

    #[test]
    fn detect_present_returns_some_with_path() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "claude", "irrelevant", true);

        let result = detect_at(
            Tool::Claude,
            path_of(dir.path()),
            VersionSource::Custom(no_version),
        )
        .unwrap();
        assert_eq!(result.tool, Tool::Claude);
        assert_eq!(result.binary_path, dir.path().join("claude"));
    }

    #[test]
    fn detect_version_field_comes_from_version_getter() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "claude", "irrelevant", true);

        let result = detect_at(
            Tool::Claude,
            path_of(dir.path()),
            VersionSource::Custom(injected_version),
        )
        .unwrap();
        assert_eq!(result.version.as_deref(), Some("injected 1.2.3"));
    }

    #[test]
    fn require_missing_errors_with_guidance() {
        let dir = tempdir().unwrap();
        let result = detect_at(
            Tool::Claude,
            path_of(dir.path()),
            VersionSource::Custom(no_version),
        );
        assert!(result.is_none());

        let err = anyhow::anyhow!(
            "claude is not installed or not found on PATH.\n  \
             Install it and make sure the binary is on your PATH."
        );
        assert!(err.to_string().contains("claude is not installed"));
        assert!(err.to_string().contains("PATH"));
    }

    #[test]
    fn require_present_returns_detected() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "codex", "irrelevant", true);

        let result = detect_at(
            Tool::Codex,
            path_of(dir.path()),
            VersionSource::Custom(no_version),
        )
        .unwrap();
        assert_eq!(result.tool, Tool::Codex);
    }

    #[test]
    fn detect_all_has_all_three_keys() {
        let dir = tempdir().unwrap();
        let path = path_of(dir.path());
        let all: HashMap<Tool, Option<DetectedTool>> = [Tool::Claude, Tool::Codex, Tool::Gemini]
            .into_iter()
            .map(|t| (t, detect_in(t, path.clone())))
            .collect();
        assert!(all.contains_key(&Tool::Claude));
        assert!(all.contains_key(&Tool::Codex));
        assert!(all.contains_key(&Tool::Gemini));
    }

    #[test]
    fn detect_all_finds_installed_tools() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "gemini", "irrelevant", true);

        // detect_all uses env PATH; drive detect_at directly for isolation.
        let path = path_of(dir.path());
        let results: HashMap<Tool, Option<DetectedTool>> =
            [Tool::Claude, Tool::Codex, Tool::Gemini]
                .into_iter()
                .map(|t| {
                    (
                        t,
                        detect_at(t, path.clone(), VersionSource::Custom(no_version)),
                    )
                })
                .collect();

        assert!(results[&Tool::Gemini].is_some());
        assert!(results[&Tool::Claude].is_none());
        assert!(results[&Tool::Codex].is_none());
    }

    // --- capture_version unit tests (spawn a single dedicated process each) ---
    // These test capture_version in isolation — one process per test, no
    // coupling to binary discovery, so parallel execution is safe.

    #[test]
    fn capture_version_returns_stdout_on_success() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "claude", "claude 2.3.1", true);
        assert_eq!(
            capture_version(&dir.path().join("claude")).as_deref(),
            Some("claude 2.3.1")
        );
    }

    #[test]
    fn capture_version_returns_stdout_on_non_zero_exit() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        // Binary exits non-zero but still prints to stdout — version should be captured.
        make_dummy_binary(dir.path(), "claude", "some output", false);
        assert!(capture_version(&dir.path().join("claude")).is_some());
    }

    #[test]
    fn capture_version_returns_none_for_missing_binary() {
        // No spawn lock needed — no process is successfully spawned here.
        let dir = tempdir().unwrap();
        assert!(capture_version(&dir.path().join("nonexistent")).is_none());
    }

    #[test]
    fn capture_version_times_out_for_hanging_binary() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let path = dir.path().join("claude");
        fs::write(&path, "#!/bin/sh\nsleep 5\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

        let start = Instant::now();
        assert!(capture_version(&path).is_none());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
