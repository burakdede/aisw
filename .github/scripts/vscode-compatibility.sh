#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
extension_dir="$repo_root/integrations/vscode"
minimum_version="${AISW_MIN_CLI_VERSION:-0.3.10}"
temporary_dir="$(mktemp -d)"
trap 'rm -rf "$temporary_dir"' EXIT

current_binary="$repo_root/target/release/aisw"
minimum_binary="$temporary_dir/aisw-$minimum_version"
minimum_checksum="$temporary_dir/aisw-$minimum_version.sha256"

case "$(uname -s):$(uname -m)" in
  Linux:x86_64) release_asset="aisw-x86_64-unknown-linux-gnu" ;;
  Linux:aarch64|Linux:arm64) release_asset="aisw-aarch64-unknown-linux-gnu" ;;
  Darwin:x86_64) release_asset="aisw-x86_64-apple-darwin" ;;
  Darwin:arm64) release_asset="aisw-aarch64-apple-darwin" ;;
  *) echo "Unsupported compatibility-gate host: $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac

echo "Building current CLI release candidate"
cargo build --release --locked --manifest-path "$repo_root/Cargo.toml"

echo "Downloading minimum supported CLI v$minimum_version"
curl --fail --silent --show-error --location \
  "https://github.com/burakdede/aisw/releases/download/v$minimum_version/$release_asset" \
  --output "$minimum_binary"
curl --fail --silent --show-error --location \
  "https://github.com/burakdede/aisw/releases/download/v$minimum_version/$release_asset.sha256" \
  --output "$minimum_checksum"
expected_checksum="$(awk '{print $1}' "$minimum_checksum")"
if command -v sha256sum >/dev/null 2>&1; then
  actual_checksum="$(sha256sum "$minimum_binary" | awk '{print $1}')"
else
  actual_checksum="$(shasum -a 256 "$minimum_binary" | awk '{print $1}')"
fi
test "$actual_checksum" = "$expected_checksum" || {
  echo "Checksum verification failed for $release_asset" >&2
  exit 1
}
chmod +x "$minimum_binary"

echo "Installing extension dependencies"
npm ci --prefix "$extension_dir"

run_compatibility() {
  local label="$1"
  local binary="$2"
  local home="$temporary_dir/home-$label"
  mkdir -p "$home"
  echo "Running extension compatibility suite against $label CLI"
  AISW_CLI_PATH="$binary" \
  AISW_HOME="$home/aisw" \
  HOME="$home" \
  npm run test:compat --prefix "$extension_dir"
}

run_compatibility current "$current_binary"
run_compatibility minimum "$minimum_binary"

echo "Running full extension tests and package inspection"
npm test --prefix "$extension_dir"
if command -v xvfb-run >/dev/null 2>&1; then
  xvfb-run -a npm run test:integration --prefix "$extension_dir"
else
  npm run test:integration --prefix "$extension_dir"
fi
npm run package --prefix "$extension_dir"

current_version_json="$($current_binary version --json)"
minimum_version_json="$($minimum_binary version --json)"
current_capabilities_json="$($current_binary capabilities --json)"
minimum_capabilities_json="$($minimum_binary capabilities --json)"
extension_version="$(node -p "require('$extension_dir/package.json').version")"
source_commit="$(git -C "$repo_root" rev-parse HEAD)"
report="$repo_root/vscode-compatibility-report.json"
jq -n \
  --arg extension_version "$extension_version" \
  --arg source_commit "$source_commit" \
  --arg minimum_release_tag "v$minimum_version" \
  --argjson current "$current_version_json" \
  --argjson minimum "$minimum_version_json" \
  --argjson current_capabilities "$current_capabilities_json" \
  --argjson minimum_capabilities "$minimum_capabilities_json" \
  '{extension_version: $extension_version, extension_commit: $source_commit, current_cli: ($current + {commit: $source_commit, capabilities: $current_capabilities}), minimum_supported_cli: ($minimum + {release_tag: $minimum_release_tag, capabilities: $minimum_capabilities})}' \
  > "$report"
echo "Compatibility report: $report"
