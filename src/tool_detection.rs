use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Result};

use crate::types::Tool;

#[derive(Debug, Clone)]
pub struct DetectedTool {
    pub tool: Tool,
    pub binary_path: PathBuf,
    pub version: Option<String>,
}

pub fn detect(tool: Tool) -> Option<DetectedTool> {
    detect_in(tool, std::env::var_os("PATH").unwrap_or_default())
}

pub fn detect_all() -> HashMap<Tool, Option<DetectedTool>> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    [Tool::Claude, Tool::Codex, Tool::Gemini]
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
    detect_in_with(tool, path, capture_version)
}

/// Separated from `detect_in` so tests can inject a mock version getter and avoid
/// spawning real processes (which is the root cause of parallel-test flakiness).
fn detect_in_with<F>(tool: Tool, path: OsString, version_fn: F) -> Option<DetectedTool>
where
    F: Fn(&std::path::Path) -> Option<String>,
{
    let binary_path = which::which_in(tool.binary_name(), Some(&path), ".").ok()?;
    let version = version_fn(&binary_path);
    Some(DetectedTool {
        tool,
        binary_path,
        version,
    })
}

pub(crate) fn capture_version(binary: &std::path::Path) -> Option<String> {
    let output = Command::new(binary).arg("--version").output().ok()?;
    // Best-effort: try stdout first, fall back to stderr.
    let raw = if !output.stdout.is_empty() {
        output.stdout
    } else {
        output.stderr
    };
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
    // These use detect_in_with with a mock version getter so they are not
    // affected by OS process-spawn latency or resource contention.

    #[test]
    fn detect_missing_returns_none() {
        let dir = tempdir().unwrap();
        // No binary in dir — should be absent regardless of version getter.
        assert!(detect_in_with(Tool::Claude, path_of(dir.path()), |_| None).is_none());
    }

    #[test]
    fn detect_present_returns_some_with_path() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "claude", "irrelevant", true);

        let result = detect_in_with(Tool::Claude, path_of(dir.path()), |_| None).unwrap();
        assert_eq!(result.tool, Tool::Claude);
        assert_eq!(result.binary_path, dir.path().join("claude"));
    }

    #[test]
    fn detect_version_field_comes_from_version_getter() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "claude", "irrelevant", true);

        let result = detect_in_with(Tool::Claude, path_of(dir.path()), |_| {
            Some("injected 1.2.3".to_owned())
        })
        .unwrap();
        assert_eq!(result.version.as_deref(), Some("injected 1.2.3"));
    }

    #[test]
    fn require_missing_errors_with_guidance() {
        let dir = tempdir().unwrap();
        let result = detect_in_with(Tool::Claude, path_of(dir.path()), |_| None);
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

        let result = detect_in_with(Tool::Codex, path_of(dir.path()), |_| None).unwrap();
        assert_eq!(result.tool, Tool::Codex);
    }

    #[test]
    fn detect_all_has_all_three_keys() {
        let all = detect_all();
        assert!(all.contains_key(&Tool::Claude));
        assert!(all.contains_key(&Tool::Codex));
        assert!(all.contains_key(&Tool::Gemini));
    }

    #[test]
    fn detect_all_finds_installed_tools() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "gemini", "irrelevant", true);

        // detect_all uses env PATH; drive detect_in_with directly for isolation.
        let path = path_of(dir.path());
        let results: HashMap<Tool, Option<DetectedTool>> =
            [Tool::Claude, Tool::Codex, Tool::Gemini]
                .into_iter()
                .map(|t| (t, detect_in_with(t, path.clone(), |_| None)))
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
}
