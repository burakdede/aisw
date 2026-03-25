#!/bin/sh
# install.sh — download and install aisw from GitHub Releases
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh
#
# Environment variables:
#   AISW_VERSION   — install a specific version (e.g. AISW_VERSION=1.0.0)
#                    defaults to the latest release
#   AISW_INSTALL_DIR — override the install directory

set -eu

REPO="burakdede/aisw"
BINARY="aisw"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

die() {
    echo "Error: $*" >&2
    exit 1
}

info() {
    echo "  $*"
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        die "'$1' is required but not found. Please install it and try again."
    fi
}

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------

detect_platform() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)
            case "$arch" in
                x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
                aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
                arm64)   TARGET="aarch64-unknown-linux-gnu" ;;
                *)       die "Unsupported Linux architecture: $arch
  Supported: x86_64, aarch64
  Install manually from https://github.com/$REPO/releases" ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64) TARGET="x86_64-apple-darwin" ;;
                arm64)  TARGET="aarch64-apple-darwin" ;;
                *)      die "Unsupported macOS architecture: $arch
  Supported: x86_64, arm64
  Install manually from https://github.com/$REPO/releases" ;;
            esac
            ;;
        *)
            die "Unsupported operating system: $os
  Supported: Linux, macOS
  On Windows, install via Cargo: cargo install aisw
  Or download manually from https://github.com/$REPO/releases"
            ;;
    esac
}

# ---------------------------------------------------------------------------
# Version resolution
# ---------------------------------------------------------------------------

resolve_version() {
    if [ -n "${AISW_VERSION:-}" ]; then
        VERSION="$AISW_VERSION"
        return
    fi

    need_cmd curl

    # Follow the redirect from /releases/latest to get the resolved tag.
    LATEST_URL="https://github.com/$REPO/releases/latest"
    REDIRECT=$(curl -fsSL --head -o /dev/null -w '%{url_effective}' "$LATEST_URL" 2>/dev/null) || \
        die "Could not determine latest version from $LATEST_URL
  Set AISW_VERSION=x.y.z to install a specific version."

    VERSION="${REDIRECT##*/}"
    # Strip leading 'v' if present.
    VERSION="${VERSION#v}"

    if [ -z "$VERSION" ]; then
        die "Could not parse version from redirect URL: $REDIRECT
  Set AISW_VERSION=x.y.z to install a specific version."
    fi
}

# ---------------------------------------------------------------------------
# Install location
# ---------------------------------------------------------------------------

resolve_install_dir() {
    if [ -n "${AISW_INSTALL_DIR:-}" ]; then
        INSTALL_DIR="$AISW_INSTALL_DIR"
        return
    fi

    if [ -w /usr/local/bin ]; then
        INSTALL_DIR="/usr/local/bin"
    elif [ "$(id -u)" -eq 0 ]; then
        # Running as root — /usr/local/bin should be writable, but wasn't.
        INSTALL_DIR="/usr/local/bin"
    else
        INSTALL_DIR="$HOME/.local/bin"
        NEEDS_PATH_NOTE=1
    fi
}

# ---------------------------------------------------------------------------
# Checksum verification
# ---------------------------------------------------------------------------

verify_checksum() {
    binary_file="$1"
    checksum_file="$2"

    if command -v sha256sum > /dev/null 2>&1; then
        # sha256sum expects "hash  filename" — rewrite the checksum file to use
        # the local basename so it passes regardless of original path.
        expected=$(awk '{print $1}' "$checksum_file")
        actual=$(sha256sum "$binary_file" | awk '{print $1}')
    elif command -v shasum > /dev/null 2>&1; then
        expected=$(awk '{print $1}' "$checksum_file")
        actual=$(shasum -a 256 "$binary_file" | awk '{print $1}')
    else
        die "No checksum tool found (sha256sum or shasum).
  Cannot verify download integrity. Aborting."
    fi

    if [ "$actual" != "$expected" ]; then
        rm -f "$binary_file" "$checksum_file"
        die "Checksum mismatch for $binary_file — download may be corrupted.
  Expected: $expected
  Got:      $actual
  The partial download has been deleted. Please try again."
    fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    need_cmd curl
    need_cmd chmod

    detect_platform
    resolve_version
    resolve_install_dir

    BINARY_NAME="${BINARY}-${TARGET}"
    BASE_URL="https://github.com/$REPO/releases/download/v${VERSION}"
    DOWNLOAD_URL="${BASE_URL}/${BINARY_NAME}"
    CHECKSUM_URL="${BASE_URL}/${BINARY_NAME}.sha256"

    echo "Installing aisw v${VERSION} (${TARGET})"
    info "Download URL: $DOWNLOAD_URL"
    info "Install dir:  $INSTALL_DIR"

    # Create a temp directory for the download.
    TMP_DIR=$(mktemp -d)
    # Clean up the temp directory on exit regardless of success/failure.
    # shellcheck disable=SC2064
    trap "rm -rf '$TMP_DIR'" EXIT

    TMP_BINARY="$TMP_DIR/$BINARY_NAME"
    TMP_CHECKSUM="$TMP_DIR/${BINARY_NAME}.sha256"

    info "Downloading binary..."
    curl -fsSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_BINARY" || \
        die "Download failed: $DOWNLOAD_URL
  Check that version v${VERSION} exists at https://github.com/$REPO/releases"

    info "Downloading checksum..."
    curl -fsSL "$CHECKSUM_URL" -o "$TMP_CHECKSUM" || \
        die "Checksum download failed: $CHECKSUM_URL"

    info "Verifying checksum..."
    verify_checksum "$TMP_BINARY" "$TMP_CHECKSUM"

    # Create the install directory if it doesn't exist (e.g. ~/.local/bin).
    if [ ! -d "$INSTALL_DIR" ]; then
        mkdir -p "$INSTALL_DIR" || die "Could not create install directory: $INSTALL_DIR"
    fi

    INSTALL_PATH="$INSTALL_DIR/$BINARY"

    # Copy with sudo if the directory is not writable.
    if [ -w "$INSTALL_DIR" ]; then
        cp "$TMP_BINARY" "$INSTALL_PATH"
        chmod +x "$INSTALL_PATH"
    else
        echo "  $INSTALL_DIR is not writable — using sudo."
        sudo cp "$TMP_BINARY" "$INSTALL_PATH"
        sudo chmod +x "$INSTALL_PATH"
    fi

    echo ""
    echo "aisw v${VERSION} installed to $INSTALL_PATH"

    if [ "${NEEDS_PATH_NOTE:-0}" -eq 1 ]; then
        echo ""
        echo "Note: $INSTALL_DIR is not in your PATH."
        echo "Add it by appending this line to your shell config:"
        echo ""
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
    fi

    echo "Run 'aisw --help' to get started."
}

main "$@"
