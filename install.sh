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
# Release selection
# ---------------------------------------------------------------------------

resolve_release_path() {
    if [ -n "${AISW_VERSION:-}" ]; then
        VERSION="$AISW_VERSION"
        RELEASE_PATH="download/v${VERSION}"
        VERSION_LABEL="v${VERSION}"
        return
    fi

    # Use GitHub's stable latest-download endpoint instead of scraping redirect
    # targets from /releases/latest. That redirect shape is not guaranteed.
    RELEASE_PATH="latest/download"
    VERSION_LABEL="latest"
}

# ---------------------------------------------------------------------------
# Install location
# ---------------------------------------------------------------------------

resolve_install_dir() {
    if [ -n "${AISW_INSTALL_DIR:-}" ]; then
        INSTALL_DIR="$AISW_INSTALL_DIR"
        return
    fi

    INSTALL_DIR="$HOME/.local/bin"
    NEEDS_PATH_NOTE=1
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
# Shell completions
# ---------------------------------------------------------------------------

install_without_sudo() {
    src="$1"
    dest="$2"

    dest_dir=$(dirname "$dest")
    if [ ! -d "$dest_dir" ]; then
        mkdir -p "$dest_dir" 2>/dev/null || return 1
    fi

    if [ ! -w "$dest_dir" ] && [ "$(id -u)" -ne 0 ]; then
        return 1
    fi

    cp "$src" "$dest"
    chmod 0644 "$dest"
}

resolve_bash_completion_path() {
    printf '%s\n' "$HOME/.local/share/bash-completion/completions/aisw"
}

resolve_zsh_completion_path() {
    if command -v zsh > /dev/null 2>&1; then
        for dir in $(zsh -fc 'print -rl -- $fpath' 2>/dev/null); do
            if [ -d "$dir" ]; then
                if [ -w "$dir" ] || [ "$(id -u)" -eq 0 ]; then
                    printf '%s\n' "$dir/_aisw"
                    return
                fi
                continue
            fi

            parent_dir=$(dirname "$dir")
            if [ -d "$parent_dir" ] && [ -w "$parent_dir" ] && mkdir -p "$dir" 2>/dev/null; then
                printf '%s\n' "$dir/_aisw"
                return
            fi
        done
    fi
    printf '%s\n' "$HOME/.zsh/completions/_aisw"
}

download_and_install_completion() {
    name="$1"
    dest="$2"
    src="$TMP_DIR/$name"
    url="${BASE_URL}/${name}"

    if curl -fsSL "$url" -o "$src"; then
        if install_without_sudo "$src" "$dest"; then
            info "Installed ${name} completion to ${dest}"
        else
            echo "Warning: could not install ${name} completion to ${dest}" >&2
        fi
    else
        echo "Warning: completion asset not found: $url" >&2
    fi
}

install_completions() {
    info "Installing shell completions..."
    download_and_install_completion "aisw.bash" "$(resolve_bash_completion_path)"
    download_and_install_completion "_aisw" "$(resolve_zsh_completion_path)"
    download_and_install_completion "aisw.fish" "$HOME/.config/fish/completions/aisw.fish"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    need_cmd curl
    need_cmd chmod

    detect_platform
    resolve_release_path
    resolve_install_dir

    BINARY_NAME="${BINARY}-${TARGET}"
    BASE_URL="https://github.com/$REPO/releases/${RELEASE_PATH}"
    DOWNLOAD_URL="${BASE_URL}/${BINARY_NAME}"
    CHECKSUM_URL="${BASE_URL}/${BINARY_NAME}.sha256"

    echo "Installing aisw ${VERSION_LABEL} (${TARGET})"
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
  Check that release ${VERSION_LABEL} exists at https://github.com/$REPO/releases"

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

    if [ ! -w "$INSTALL_DIR" ] && [ "$(id -u)" -ne 0 ]; then
        die "Install directory is not writable: $INSTALL_DIR
  Set AISW_INSTALL_DIR to a user-writable path and re-run the installer."
    fi

    cp "$TMP_BINARY" "$INSTALL_PATH"
    chmod +x "$INSTALL_PATH"

    echo ""
    echo "aisw ${VERSION_LABEL} installed to $INSTALL_PATH"

    install_completions

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
