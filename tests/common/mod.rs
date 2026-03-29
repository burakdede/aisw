#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// Sandboxed environment for integration tests.
///
/// Every test gets its own temp dir used as both AISW_HOME and a fake PATH
/// containing dummy tool binaries. Nothing touches the developer's real home
/// directory or installed tools.
pub struct TestEnv {
    pub dir: TempDir,
    pub aisw_home: PathBuf,
    pub bin_dir: PathBuf,
    /// Fake HOME dir — set as HOME env var so tools that use dirs::home_dir()
    /// (e.g. Gemini .env rewrite) write to a sandboxed location.
    pub fake_home: PathBuf,
}

impl TestEnv {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("failed to create temp dir");
        let aisw_home = dir.path().join("aisw_home");
        let bin_dir = dir.path().join("bin");
        let fake_home = dir.path().join("home");
        fs::create_dir_all(&aisw_home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&fake_home).unwrap();
        Self {
            dir,
            aisw_home,
            bin_dir,
            fake_home,
        }
    }

    /// Add a fake binary to the sandboxed PATH that prints `version_output` and exits 0.
    pub fn add_fake_tool(&self, name: &str, version_output: &str) {
        self.add_fake_tool_with_exit(name, version_output, 0);
    }

    pub fn add_fake_tool_with_exit(&self, name: &str, version_output: &str, exit_code: i32) {
        let path = self.bin_dir.join(name);
        fs::write(
            &path,
            format!("#!/bin/sh\necho '{}'\nexit {}\n", version_output, exit_code),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Returns an `assert_cmd::Command` for `aisw` pre-configured with the
    /// sandboxed AISW_HOME and PATH.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("aisw").expect("aisw binary not found");
        cmd.env("AISW_HOME", &self.aisw_home)
            .env("PATH", &self.bin_dir)
            .env("HOME", &self.fake_home);
        cmd
    }

    pub fn output(&self, args: &[&str]) -> std::process::Output {
        self.cmd()
            .args(args)
            .output()
            .unwrap_or_else(|_| panic!("command failed to launch: {}", args.join(" ")))
    }

    /// Convenience: path to a file inside AISW_HOME.
    pub fn home_file(&self, rel: &str) -> PathBuf {
        self.aisw_home.join(rel)
    }

    /// Read a file inside AISW_HOME.
    pub fn read_home_file(&self, rel: &str) -> String {
        fs::read_to_string(self.home_file(rel))
            .unwrap_or_else(|_| panic!("file not found: {}", rel))
    }

    /// Assert a file inside AISW_HOME exists.
    pub fn assert_home_file_exists(&self, rel: &str) {
        assert!(
            self.home_file(rel).exists(),
            "expected file to exist: {}",
            rel
        );
    }

    /// Assert a file inside AISW_HOME has 0600 permissions.
    pub fn assert_file_is_600(&self, path: &Path) {
        let mode = fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "expected 0600 on {}, got {:o}",
            path.display(),
            mode & 0o777
        );
    }
}

pub fn assert_output_redacts_secret(output: &std::process::Output, secret: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        !combined.contains(secret),
        "full secret leaked in output\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
    );

    let fragment = secret_fragment(secret);
    if !fragment.is_empty() {
        assert!(
            !combined.contains(fragment),
            "recognizable secret fragment leaked in output: {fragment}\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
        );
    }
}

fn secret_fragment(secret: &str) -> &str {
    let start = secret.len() / 3;
    let end = (start + 10).min(secret.len());
    &secret[start..end]
}
