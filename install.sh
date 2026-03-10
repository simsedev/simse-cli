#!/bin/sh
# SimSE CLI installer
# Usage: curl -fsSL https://simse.dev/install | sh
#
# Installs the latest simse binary to /usr/local/bin (or ~/.local/bin if
# /usr/local/bin is not writable).

set -e

REPO="simsedev/simse-cli"
BINARY_NAME="simse"
INSTALL_DIR="/usr/local/bin"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

info() {
    printf '\033[1;34m%s\033[0m\n' "$1"
}

error() {
    printf '\033[1;31merror: %s\033[0m\n' "$1" >&2
    exit 1
}

# ---------------------------------------------------------------------------
# Detect OS and architecture
# ---------------------------------------------------------------------------

detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)  OS="linux" ;;
        Darwin) OS="darwin" ;;
        *)      error "unsupported OS: $OS" ;;
    esac

    case "$ARCH" in
        x86_64|amd64)   ARCH="x86_64" ;;
        aarch64|arm64)  ARCH="aarch64" ;;
        *)              error "unsupported architecture: $ARCH" ;;
    esac

    PLATFORM="${OS}-${ARCH}"
}

# ---------------------------------------------------------------------------
# Find latest release
# ---------------------------------------------------------------------------

get_latest_version() {
    if command -v curl >/dev/null 2>&1; then
        VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')"
    elif command -v wget >/dev/null 2>&1; then
        VERSION="$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')"
    else
        error "curl or wget is required"
    fi

    if [ -z "$VERSION" ]; then
        error "could not determine latest version"
    fi
}

# ---------------------------------------------------------------------------
# Download and install
# ---------------------------------------------------------------------------

download_and_install() {
    FILENAME="simse-${PLATFORM}.tar.gz"
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${FILENAME}"

    info "downloading simse ${VERSION} for ${PLATFORM}..."

    TMPDIR="$(mktemp -d)"
    trap 'rm -rf "$TMPDIR"' EXIT

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$URL" -o "${TMPDIR}/${FILENAME}"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$URL" -O "${TMPDIR}/${FILENAME}"
    fi

    tar -xzf "${TMPDIR}/${FILENAME}" -C "$TMPDIR"

    # Choose install location
    if [ -w "$INSTALL_DIR" ]; then
        TARGET="$INSTALL_DIR"
    elif [ -w "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
        TARGET="$HOME/.local/bin"
    else
        info "installing to ${INSTALL_DIR} (requires sudo)..."
        sudo mv "${TMPDIR}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
        sudo chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
        info "simse ${VERSION} installed to ${INSTALL_DIR}/${BINARY_NAME}"
        return
    fi

    mv "${TMPDIR}/${BINARY_NAME}" "${TARGET}/${BINARY_NAME}"
    chmod +x "${TARGET}/${BINARY_NAME}"
    info "simse ${VERSION} installed to ${TARGET}/${BINARY_NAME}"

    # Check if target is in PATH
    case ":$PATH:" in
        *":${TARGET}:"*) ;;
        *)
            printf '\033[1;33mwarning:\033[0m %s is not in your PATH\n' "$TARGET"
            echo "  Add it with: export PATH=\"${TARGET}:\$PATH\""
            ;;
    esac
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    detect_platform
    get_latest_version
    download_and_install
    info "run 'simse' to get started"
}

main
