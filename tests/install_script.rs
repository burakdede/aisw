use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const TEST_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct InstallerEnv {
    _dir: TempDir,
    home_dir: PathBuf,
    curl_log: PathBuf,
    path: String,
    assets_dir: PathBuf,
    os: String,
    arch: String,
}

impl InstallerEnv {
    fn new(os: &str, arch: &str) -> Self {
        let dir = TempDir::new().expect("failed to create temp dir");
        let stub_dir = dir.path().join("bin");
        let home_dir = dir.path().join("home");
        let assets_dir = dir.path().join("assets");
        let curl_log = dir.path().join("curl.log");

        fs::create_dir_all(&stub_dir).unwrap();
        fs::create_dir_all(&home_dir).unwrap();
        fs::create_dir_all(&assets_dir).unwrap();
        fs::write(&curl_log, "").unwrap();

        for target in [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ] {
            fs::write(
                assets_dir.join(format!("aisw-{target}")),
                "#!/bin/sh\necho fake aisw\n",
            )
            .unwrap();
            fs::write(
                assets_dir.join(format!("aisw-{target}.sha256")),
                format!("{TEST_SHA256}\n"),
            )
            .unwrap();
        }
        fs::write(
            assets_dir.join("aisw.bash"),
            "# bash completion\nclaude codex gemini\n",
        )
        .unwrap();
        fs::write(assets_dir.join("_aisw"), "#compdef aisw\n").unwrap();
        fs::write(assets_dir.join("aisw.fish"), "# fish completion\n").unwrap();

        write_executable(
            &stub_dir.join("curl"),
            r#"#!/bin/sh
set -eu
log_file="${AISW_TEST_CURL_LOG:?}"
assets_dir="${AISW_TEST_ASSETS_DIR:?}"
out=""
url=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o)
            shift
            out="$1"
            ;;
        http://*|https://*)
            url="$1"
            ;;
    esac
    shift
done
printf '%s\n' "$url" >> "$log_file"
asset_name=$(basename "$url")
cp "$assets_dir/$asset_name" "$out"
"#,
        );
        write_executable(
            &stub_dir.join("uname"),
            format!(
                r#"#!/bin/sh
set -eu
case "${{1:-}}" in
    -s) echo "{os}" ;;
    -m) echo "{arch}" ;;
    *) echo "{os}" ;;
esac
"#,
            )
            .as_str(),
        );
        write_executable(
            &stub_dir.join("sha256sum"),
            format!("#!/bin/sh\nset -eu\nprintf '%s  %s\\n' \"{TEST_SHA256}\" \"$1\"\n").as_str(),
        );
        write_executable(
            &stub_dir.join("zsh"),
            r#"#!/bin/sh
set -eu
printf '%s\n' "$HOME/.zfunc"
"#,
        );

        let base_path = std::env::var("PATH").unwrap_or_default();
        let path = if base_path.is_empty() {
            stub_dir.display().to_string()
        } else {
            format!("{}:{}", stub_dir.display(), base_path)
        };

        Self {
            _dir: dir,
            home_dir,
            curl_log,
            path,
            assets_dir,
            os: os.to_owned(),
            arch: arch.to_owned(),
        }
    }

    fn run(&self, version: Option<&str>, install_dir: Option<&Path>) -> std::process::Output {
        let mut cmd = Command::new("sh");
        cmd.arg(manifest_path("install.sh"))
            .env("PATH", &self.path)
            .env("HOME", &self.home_dir)
            .env("AISW_TEST_CURL_LOG", &self.curl_log)
            .env("AISW_TEST_ASSETS_DIR", &self.assets_dir);

        if let Some(install_dir) = install_dir {
            cmd.env("AISW_INSTALL_DIR", install_dir);
        }

        if let Some(version) = version {
            cmd.env("AISW_VERSION", version);
        }

        cmd.output().expect("failed to run install.sh")
    }

    fn curl_urls(&self) -> Vec<String> {
        fs::read_to_string(&self.curl_log)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn default_install_dir(&self) -> PathBuf {
        self.home_dir.join(".local/bin")
    }

    fn default_install_path(&self) -> PathBuf {
        self.default_install_dir().join("aisw")
    }

    fn target(&self) -> &'static str {
        match (self.os.as_str(), self.arch.as_str()) {
            ("Linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("Linux", "aarch64") | ("Linux", "arm64") => "aarch64-unknown-linux-gnu",
            ("Darwin", "x86_64") => "x86_64-apple-darwin",
            ("Darwin", "arm64") => "aarch64-apple-darwin",
            _ => panic!("unsupported test platform"),
        }
    }
}

fn manifest_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn install_script_uses_latest_download_endpoint_by_default() {
    let env = InstallerEnv::new("Linux", "x86_64");
    let custom_install_dir = env.home_dir.join("install");
    fs::create_dir_all(&custom_install_dir).unwrap();
    let output = env.run(None, Some(&custom_install_dir));

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let urls = env.curl_urls();
    assert_eq!(urls.len(), 5);
    assert_eq!(
        urls[0],
        "https://github.com/burakdede/aisw/releases/latest/download/aisw-x86_64-unknown-linux-gnu"
    );
    assert_eq!(
        urls[1],
        "https://github.com/burakdede/aisw/releases/latest/download/aisw-x86_64-unknown-linux-gnu.sha256"
    );
    assert_eq!(
        urls[2],
        "https://github.com/burakdede/aisw/releases/latest/download/aisw.bash"
    );
    assert_eq!(
        urls[3],
        "https://github.com/burakdede/aisw/releases/latest/download/_aisw"
    );
    assert_eq!(
        urls[4],
        "https://github.com/burakdede/aisw/releases/latest/download/aisw.fish"
    );
    assert!(custom_install_dir.join("aisw").exists());
    assert!(env
        .home_dir
        .join(".local/share/bash-completion/completions/aisw")
        .exists());
    assert!(env.home_dir.join(".zfunc/_aisw").exists());
    assert!(env
        .home_dir
        .join(".config/fish/completions/aisw.fish")
        .exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Installing aisw latest"));
    assert!(!stdout.contains("vreleases"));
}

#[test]
fn install_script_uses_pinned_version_when_aisw_version_is_set() {
    let env = InstallerEnv::new("Linux", "x86_64");
    let custom_install_dir = env.home_dir.join("install");
    fs::create_dir_all(&custom_install_dir).unwrap();
    let output = env.run(Some("1.2.3"), Some(&custom_install_dir));

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let urls = env.curl_urls();
    assert_eq!(urls.len(), 5);
    assert_eq!(
        urls[0],
        "https://github.com/burakdede/aisw/releases/download/v1.2.3/aisw-x86_64-unknown-linux-gnu"
    );
    assert_eq!(
        urls[1],
        "https://github.com/burakdede/aisw/releases/download/v1.2.3/aisw-x86_64-unknown-linux-gnu.sha256"
    );
    assert_eq!(
        urls[2],
        "https://github.com/burakdede/aisw/releases/download/v1.2.3/aisw.bash"
    );
    assert_eq!(
        urls[3],
        "https://github.com/burakdede/aisw/releases/download/v1.2.3/_aisw"
    );
    assert_eq!(
        urls[4],
        "https://github.com/burakdede/aisw/releases/download/v1.2.3/aisw.fish"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Installing aisw v1.2.3"));
}

#[test]
fn install_script_defaults_to_home_local_bin() {
    let env = InstallerEnv::new("Darwin", "arm64");
    let output = env.run(None, None);

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(env.default_install_path().exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!(
        "Install dir:  {}",
        env.default_install_dir().display()
    )));
    assert!(stdout.contains("Note:"));
}

#[test]
fn install_script_uses_expected_target_for_supported_platforms() {
    for (os, arch) in [
        ("Linux", "x86_64"),
        ("Linux", "aarch64"),
        ("Linux", "arm64"),
        ("Darwin", "x86_64"),
        ("Darwin", "arm64"),
    ] {
        let env = InstallerEnv::new(os, arch);
        let expected_target = env.target();
        let output = env.run(None, None);

        assert!(
            output.status.success(),
            "platform {os}/{arch} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let urls = env.curl_urls();
        assert_eq!(
            urls[0],
            format!(
                "https://github.com/burakdede/aisw/releases/latest/download/aisw-{expected_target}"
            )
        );
        assert_eq!(
            urls[1],
            format!(
                "https://github.com/burakdede/aisw/releases/latest/download/aisw-{expected_target}.sha256"
            )
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(&format!("Installing aisw latest ({expected_target})")));
        assert!(env.default_install_path().exists());
    }
}
