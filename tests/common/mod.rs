#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
        #[cfg(unix)]
        {
            fs::write(
                &path,
                format!("#!/bin/sh\necho '{}'\nexit {}\n", version_output, exit_code),
            )
            .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(windows)]
        {
            let script = self.bin_dir.join(format!("{name}.cmd"));
            fs::write(
                &script,
                format!(
                    "@echo off\r\necho {}\r\nexit /b {}\r\n",
                    version_output, exit_code
                ),
            )
            .unwrap();
            let _ = path;
        }
    }

    pub fn add_script_tool(&self, name: &str, script: &str) {
        #[cfg(unix)]
        {
            let path = self.bin_dir.join(name);
            fs::write(&path, script).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(windows)]
        {
            let script_path = self.bin_dir.join(format!("{name}.cmd"));
            fs::write(&script_path, script).unwrap();
        }
    }

    /// Returns an `assert_cmd::Command` for `aisw` pre-configured with the
    /// sandboxed AISW_HOME and PATH.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("aisw").expect("aisw binary not found");
        cmd.env("AISW_HOME", &self.aisw_home)
            .env("PATH", &self.bin_dir)
            .env("HOME", &self.fake_home)
            .env("AISW_KEYRING_TEST_DIR", self.fake_home.join("keychain"))
            // Prevent ambient developer env from redirecting writes into real homes/config.
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_DATA_HOME")
            .env_remove("AISW_SECURITY_BIN")
            .env_remove("AISW_SECURITY_KEYCHAIN")
            .env_remove("AISW_CLAUDE_AUTH_STORAGE")
            .env_remove("AISW_CODEX_AUTH_STORAGE");
        #[cfg(windows)]
        {
            let roaming = self.fake_home.join("AppData").join("Roaming");
            let local = self.fake_home.join("AppData").join("Local");
            fs::create_dir_all(&roaming).expect("failed to create fake AppData/Roaming");
            fs::create_dir_all(&local).expect("failed to create fake AppData/Local");
            cmd.env("USERPROFILE", &self.fake_home)
                .env("APPDATA", roaming)
                .env("LOCALAPPDATA", local);
        }
        cmd
    }

    pub fn output(&self, args: &[&str]) -> std::process::Output {
        self.cmd()
            .args(args)
            .output()
            .unwrap_or_else(|_| panic!("command failed to launch: {}", args.join(" ")))
    }

    pub fn aisw_bin(&self) -> PathBuf {
        assert_cmd::cargo::cargo_bin("aisw")
    }

    pub fn shell_path(&self) -> String {
        let aisw_bin = self.aisw_bin();
        let aisw_dir = aisw_bin
            .parent()
            .expect("aisw binary should have a parent directory");
        // Keep shell tests deterministic: prefer test bins + cargo target dir,
        // then only baseline system paths (not user-local PATH entries).
        format!(
            "{}:{}:/usr/bin:/bin:/usr/sbin:/sbin",
            self.bin_dir.display(),
            aisw_dir.display(),
        )
    }

    pub fn shell_cmd(&self, shell: &str) -> Option<StdCommand> {
        let mut cmd = match shell {
            "bash" => {
                let mut cmd = StdCommand::new("bash");
                cmd.args(["--noprofile", "--norc"]);
                cmd
            }
            "zsh" => {
                let mut cmd = StdCommand::new("zsh");
                cmd.arg("-f");
                cmd
            }
            "fish" => {
                let mut cmd = StdCommand::new("fish");
                cmd.arg("--no-config");
                cmd
            }
            _ => panic!("unsupported shell: {shell}"),
        };

        cmd.env("AISW_HOME", &self.aisw_home)
            .env("HOME", &self.fake_home)
            .env("PATH", self.shell_path())
            .env("AISW_KEYRING_TEST_DIR", self.fake_home.join("keychain"))
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_DATA_HOME")
            .env_remove("AISW_SECURITY_BIN")
            .env_remove("AISW_SECURITY_KEYCHAIN")
            .env_remove("AISW_CLAUDE_AUTH_STORAGE")
            .env_remove("AISW_CODEX_AUTH_STORAGE");

        Some(cmd)
    }

    pub fn run_shell_script(&self, shell: &str, script: &str) -> Option<Output> {
        let mut cmd = self.shell_cmd(shell)?;
        cmd.args(["-c", script]).output().ok()
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
        #[cfg(unix)]
        {
            let mode = fs::metadata(path).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "expected 0600 on {}, got {:o}",
                path.display(),
                mode & 0o777
            );
        }
        #[cfg(windows)]
        {
            let _ = path;
        }
    }
}

/// Installs a fake `security` binary for testing Claude Code's macOS Keychain
/// integration. The mock stores items as plain files under `AISW_KEYRING_TEST_DIR`
/// and supports `find-generic-password` and `add-generic-password`.
pub fn add_fake_security_tool(env: &TestEnv) {
    env.add_script_tool(
        "security",
        "#!/bin/sh\n\
         store_root=\"${AISW_KEYRING_TEST_DIR:-$HOME/keychain}\"\n\
         item_dir() {\n\
           printf '%s/%s/%s' \"$store_root\" \"$1\" \"$2\"\n\
         }\n\
         first_item_dir() {\n\
           dir=\"$store_root/$1\"\n\
           [ -d \"$dir\" ] || return 1\n\
           for item in \"$dir\"/*; do\n\
             [ -d \"$item\" ] || continue\n\
             printf '%s' \"$item\"\n\
             return 0\n\
           done\n\
           return 1\n\
         }\n\
         cmd=\"$1\"\n\
         shift\n\
         service=''\n\
         account=''\n\
         case \"$cmd\" in\n\
           find-generic-password)\n\
             while [ \"$#\" -gt 0 ]; do\n\
               case \"$1\" in\n\
                 -s)\n\
                   shift\n\
                   service=\"$1\"\n\
                   ;;\n\
                 -a)\n\
                   shift\n\
                   account=\"$1\"\n\
                   ;;\n\
               esac\n\
               shift\n\
             done\n\
             if [ -n \"$account\" ]; then\n\
               item=\"$(item_dir \"$service\" \"$account\")\"\n\
             else\n\
               item=\"$(first_item_dir \"$service\")\" || item=''\n\
             fi\n\
             if [ -f \"$item/secret\" ]; then\n\
               /bin/cat \"$item/secret\"\n\
               exit 0\n\
             fi\n\
             echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
             exit 44\n\
             ;;\n\
           add-generic-password)\n\
             while [ \"$#\" -gt 0 ]; do\n\
               case \"$1\" in\n\
                 -s)\n\
                   shift\n\
                   service=\"$1\"\n\
                   ;;\n\
                 -a)\n\
                   shift\n\
                   account=\"$1\"\n\
                   ;;\n\
                 -w)\n\
                   shift\n\
                   item=\"$(item_dir \"$service\" \"$account\")\"\n\
                   /bin/mkdir -p \"$item\"\n\
                   printf '%s' \"$account\" > \"$item/account\"\n\
                   if [ \"$#\" -gt 0 ] && [ \"${1#-}\" = \"$1\" ]; then\n\
                     printf '%s' \"$1\" > \"$item/secret\"\n\
                     exit 0\n\
                   else\n\
                     IFS= read -r secret || true\n\
                     printf '%s' \"$secret\" > \"$item/secret\"\n\
                     exit 0\n\
                   fi\n\
                   ;;\n\
                 *)\n\
                   shift\n\
                   ;;\n\
               esac\n\
             done\n\
             echo 'missing -w password' >&2\n\
             exit 1\n\
             ;;\n\
           *)\n\
             echo \"unexpected security command: $cmd\" >&2\n\
             exit 1\n\
             ;;\n\
         esac\n",
    );
}

/// Installs a fake `security` binary for testing Codex keyring integration.
/// Uses the same file-backed store as `add_fake_security_tool` but omits the
/// `-T` trusted-app flag that is specific to Claude's macOS Keychain writes.
pub fn add_fake_codex_security_tool(env: &TestEnv) {
    env.add_script_tool(
        "security",
        "#!/bin/sh\n\
         store_root=\"${AISW_KEYRING_TEST_DIR:-$HOME/keychain}\"\n\
         item_dir() {\n\
           printf '%s/%s/%s' \"$store_root\" \"$1\" \"$2\"\n\
         }\n\
         first_item_dir() {\n\
           dir=\"$store_root/$1\"\n\
           [ -d \"$dir\" ] || return 1\n\
           for item in \"$dir\"/*; do\n\
             [ -d \"$item\" ] || continue\n\
             printf '%s' \"$item\"\n\
             return 0\n\
           done\n\
           return 1\n\
         }\n\
         cmd=\"$1\"\n\
         shift\n\
         case \"$cmd\" in\n\
           find-generic-password)\n\
             service=''\n\
             account=''\n\
             while [ \"$#\" -gt 0 ]; do\n\
               case \"$1\" in\n\
                 -s)\n\
                   shift\n\
                   service=\"$1\"\n\
                   ;;\n\
                 -a)\n\
                   shift\n\
                   account=\"$1\"\n\
                   ;;\n\
               esac\n\
               shift\n\
             done\n\
             if [ -n \"$account\" ]; then\n\
               item=\"$(item_dir \"$service\" \"$account\")\"\n\
             else\n\
               item=\"$(first_item_dir \"$service\")\" || item=''\n\
             fi\n\
             if [ -f \"$item/secret\" ]; then\n\
               /bin/cat \"$item/secret\"\n\
               exit 0\n\
             fi\n\
             echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
             exit 44\n\
             ;;\n\
           add-generic-password)\n\
             service=''\n\
             account=''\n\
             while [ \"$#\" -gt 0 ]; do\n\
               case \"$1\" in\n\
                 -s)\n\
                   shift\n\
                   service=\"$1\"\n\
                   ;;\n\
                 -a)\n\
                   shift\n\
                   account=\"$1\"\n\
                   ;;\n\
                 -w)\n\
                   shift\n\
                   item=\"$(item_dir \"$service\" \"$account\")\"\n\
                   /bin/mkdir -p \"$item\"\n\
                   printf '%s' \"$account\" > \"$item/account\"\n\
                   if [ \"$#\" -gt 0 ] && [ \"${1#-}\" = \"$1\" ]; then\n\
                     secret=\"$1\"\n\
                   else\n\
                     IFS= read -r secret || true\n\
                   fi\n\
                   printf '%s' \"$secret\" > \"$item/secret\"\n\
                   exit 0\n\
                   ;;\n\
               esac\n\
               shift\n\
             done\n\
             echo 'missing -w password' >&2\n\
             exit 1\n\
             ;;\n\
           *)\n\
             echo \"unexpected security command: $cmd\" >&2\n\
             exit 1\n\
             ;;\n\
         esac\n",
    );
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
