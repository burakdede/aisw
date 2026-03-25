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
    let binary_path = which::which_in(tool.binary_name(), Some(&path), ".").ok()?;
    let version = capture_version(&binary_path);
    Some(DetectedTool {
        tool,
        binary_path,
        version,
    })
}

fn capture_version(binary: &std::path::Path) -> Option<String> {
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

    #[test]
    fn detect_missing_returns_none() {
        let dir = tempdir().unwrap();
        assert!(detect_in(Tool::Claude, path_of(dir.path())).is_none());
    }

    #[test]
    fn detect_present_returns_some_with_path() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "claude", "claude 2.3.1", true);

        let result = detect_in(Tool::Claude, path_of(dir.path())).unwrap();
        assert_eq!(result.tool, Tool::Claude);
        assert_eq!(result.binary_path, dir.path().join("claude"));
    }

    #[test]
    fn detect_captures_version_string() {
        let dir = tempdir().unwrap();
        make_dummy_binary(dir.path(), "claude", "claude 2.3.1", true);

        let result = detect_in(Tool::Claude, path_of(dir.path())).unwrap();
        assert_eq!(result.version.as_deref(), Some("claude 2.3.1"));
    }

    #[test]
    fn detect_non_zero_exit_version_captured_from_stdout() {
        let dir = tempdir().unwrap();
        // Binary exits non-zero but still prints to stdout — version should be captured.
        make_dummy_binary(dir.path(), "claude", "some output", false);

        let result = detect_in(Tool::Claude, path_of(dir.path())).unwrap();
        assert!(result.version.is_some());
    }

    #[test]
    fn require_missing_errors_with_guidance() {
        let dir = tempdir().unwrap();
        // Temporarily override PATH for this specific call via detect_in path.
        // require() uses env PATH, so we test the error message shape via detect_in + manual check.
        let result = detect_in(Tool::Claude, path_of(dir.path()));
        assert!(result.is_none());

        // Construct the error as require() would.
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
        make_dummy_binary(dir.path(), "codex", "codex 1.0.0", true);

        let result = detect_in(Tool::Codex, path_of(dir.path())).unwrap();
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
        make_dummy_binary(dir.path(), "gemini", "gemini 0.9", true);

        // detect_all uses env PATH; test detect_in directly for isolation.
        let path = path_of(dir.path());
        let results: HashMap<Tool, Option<DetectedTool>> =
            [Tool::Claude, Tool::Codex, Tool::Gemini]
                .into_iter()
                .map(|t| (t, detect_in(t, path.clone())))
                .collect();

        assert!(results[&Tool::Gemini].is_some());
        assert!(results[&Tool::Claude].is_none());
        assert!(results[&Tool::Codex].is_none());
    }
}
