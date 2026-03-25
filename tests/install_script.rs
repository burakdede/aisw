use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const TEST_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct InstallerEnv {
    _dir: TempDir,
    install_dir: PathBuf,
    curl_log: PathBuf,
    path: String,
    binary_src: PathBuf,
    checksum_src: PathBuf,
}

impl InstallerEnv {
    fn new() -> Self {
        let dir = TempDir::new().expect("failed to create temp dir");
        let stub_dir = dir.path().join("bin");
        let install_dir = dir.path().join("install");
        let assets_dir = dir.path().join("assets");
        let curl_log = dir.path().join("curl.log");

        fs::create_dir_all(&stub_dir).unwrap();
        fs::create_dir_all(&install_dir).unwrap();
        fs::create_dir_all(&assets_dir).unwrap();
        fs::write(&curl_log, "").unwrap();

        let binary_src = assets_dir.join("aisw-x86_64-unknown-linux-gnu");
        fs::write(&binary_src, "#!/bin/sh\necho fake aisw\n").unwrap();

        let checksum_src = assets_dir.join("aisw-x86_64-unknown-linux-gnu.sha256");
        fs::write(&checksum_src, format!("{TEST_SHA256}\n")).unwrap();

        write_executable(
            &stub_dir.join("curl"),
            r#"#!/bin/sh
set -eu
log_file="${AISW_TEST_CURL_LOG:?}"
binary_src="${AISW_TEST_BINARY_SRC:?}"
checksum_src="${AISW_TEST_CHECKSUM_SRC:?}"
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
case "$url" in
    *.sha256) cp "$checksum_src" "$out" ;;
    *) cp "$binary_src" "$out" ;;
esac
"#,
        );
        write_executable(
            &stub_dir.join("uname"),
            r#"#!/bin/sh
set -eu
case "${1:-}" in
    -s) echo Linux ;;
    -m) echo x86_64 ;;
    *) echo Linux ;;
esac
"#,
        );
        write_executable(
            &stub_dir.join("sha256sum"),
            format!("#!/bin/sh\nset -eu\nprintf '%s  %s\\n' \"{TEST_SHA256}\" \"$1\"\n").as_str(),
        );

        let base_path = std::env::var("PATH").unwrap_or_default();
        let path = if base_path.is_empty() {
            stub_dir.display().to_string()
        } else {
            format!("{}:{}", stub_dir.display(), base_path)
        };

        Self {
            _dir: dir,
            install_dir,
            curl_log,
            path,
            binary_src,
            checksum_src,
        }
    }

    fn run(&self, version: Option<&str>) -> std::process::Output {
        let mut cmd = Command::new("sh");
        cmd.arg(manifest_path("install.sh"))
            .env("PATH", &self.path)
            .env("AISW_INSTALL_DIR", &self.install_dir)
            .env("AISW_TEST_CURL_LOG", &self.curl_log)
            .env("AISW_TEST_BINARY_SRC", &self.binary_src)
            .env("AISW_TEST_CHECKSUM_SRC", &self.checksum_src);

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
    let env = InstallerEnv::new();
    let output = env.run(None);

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let urls = env.curl_urls();
    assert_eq!(urls.len(), 2);
    assert_eq!(
        urls[0],
        "https://github.com/burakdede/aisw/releases/latest/download/aisw-x86_64-unknown-linux-gnu"
    );
    assert_eq!(
        urls[1],
        "https://github.com/burakdede/aisw/releases/latest/download/aisw-x86_64-unknown-linux-gnu.sha256"
    );
    assert!(env.install_dir.join("aisw").exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Installing aisw latest"));
    assert!(!stdout.contains("vreleases"));
}

#[test]
fn install_script_uses_pinned_version_when_aisw_version_is_set() {
    let env = InstallerEnv::new();
    let output = env.run(Some("1.2.3"));

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let urls = env.curl_urls();
    assert_eq!(urls.len(), 2);
    assert_eq!(
        urls[0],
        "https://github.com/burakdede/aisw/releases/download/v1.2.3/aisw-x86_64-unknown-linux-gnu"
    );
    assert_eq!(
        urls[1],
        "https://github.com/burakdede/aisw/releases/download/v1.2.3/aisw-x86_64-unknown-linux-gnu.sha256"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Installing aisw v1.2.3"));
}
