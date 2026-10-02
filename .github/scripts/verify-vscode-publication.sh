#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: verify-vscode-publication.sh <version> <vsix> }"
vsix="${2:?usage: verify-vscode-publication.sh <version> <vsix> }"
namespace="${VSCE_PUBLISHER:-aisw}"
extension_name="${VSCE_EXTENSION_NAME:-aisw-vscode}"
open_vsx_api="${OPEN_VSX_API:-https://open-vsx.org/api}"
marketplace_api="${VS_MARKETPLACE_API:-https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery}"
temporary_dir="$(mktemp -d)"
trap 'rm -rf "$temporary_dir"' EXIT

checksum() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

expected_checksum="$(checksum "$vsix")"
expected_manifest="$(unzip -p "$vsix" extension/package.json | jq -S .)"

retry() {
  local attempts=20
  local delay=15
  local attempt
  for attempt in $(seq 1 "$attempts"); do
    if "$@"; then return 0; fi
    if [ "$attempt" -lt "$attempts" ]; then sleep "$delay"; fi
  done
  echo "Timed out waiting for marketplace publication." >&2
  return 1
}

open_vsx_json="$temporary_dir/open-vsx.json"
retry curl --fail --silent --show-error --location \
  "$open_vsx_api/$namespace/$extension_name" > "$open_vsx_json"
test "$(jq -r '.version' "$open_vsx_json")" = "$version" || {
  echo "Open VSX version does not match $version" >&2
  exit 1
}
open_vsx_download="$(jq -r '.files.download // empty' "$open_vsx_json")"
test -n "$open_vsx_download" || { echo "Open VSX did not return a download URL" >&2; exit 1; }

marketplace_body="$(jq -nc --arg extension "$namespace.$extension_name" '{filters:[{criteria:[{filterType:7,value:$extension}]}],flags:914}')"
marketplace_json="$temporary_dir/marketplace.json"
retry curl --fail --silent --show-error --location --request POST "$marketplace_api" \
  --header 'Content-Type: application/json' \
  --header 'Accept: application/json;api-version=3.0-preview.1' \
  --data "$marketplace_body" > "$marketplace_json"
test "$(jq -r '.results[0].extensions[0].versions[0].version // empty' "$marketplace_json")" = "$version" || {
  echo "Visual Studio Marketplace version does not match $version" >&2
  exit 1
}
marketplace_download="$(jq -r '.results[0].extensions[0].versions[0].files[] | select(.assetType == "Microsoft.VisualStudio.Services.VSIXPackage") | .source' "$marketplace_json" | head -n1)"
test -n "$marketplace_download" || { echo "Visual Studio Marketplace did not return a VSIX URL" >&2; exit 1; }

for registry in open-vsx marketplace; do
  destination="$temporary_dir/$registry.vsix"
  if [ "$registry" = open-vsx ]; then
    curl --fail --silent --show-error --location "$open_vsx_download" --output "$destination"
  else
    curl --fail --silent --show-error --location "$marketplace_download" --output "$destination"
  fi
  unzip -tqq "$destination"
  actual_manifest="$(unzip -p "$destination" extension/package.json | jq -S .)"
  test "$actual_manifest" = "$expected_manifest" || {
    echo "$registry VSIX manifest does not match the tested artifact" >&2
    exit 1
  }
  echo "$registry VSIX SHA-256: $(checksum "$destination")"
done

echo "Verified $namespace.$extension_name@$version on Open VSX and Visual Studio Marketplace."
echo "VSIX SHA-256: $expected_checksum"
