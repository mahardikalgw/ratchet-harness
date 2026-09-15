#!/bin/sh
# Ratchet installer.
#
#   curl -fsSL https://raw.githubusercontent.com/mahardikalgw/ratchet-harness/main/install.sh | sh
#
# Environment variables:
#   RATCHET_VERSION      Release tag to install, e.g. v0.1.0 (default: latest)
#   RATCHET_INSTALL_DIR  Target directory (default: ~/.local/bin, or /usr/local/bin if writable)
#
# This script downloads a prebuilt binary and verifies its SHA256 checksum when
# the release publishes one. It never runs sudo unless it has to.

set -eu

REPO="mahardikalgw/ratchet-harness"
BIN="ratchet"
VERSION="${RATCHET_VERSION:-latest}"

err() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

info() {
    printf '%s\n' "$1"
}

# ----- detect platform -----

detect_target() {
    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Linux)  os_part="unknown-linux-musl" ;;
        Darwin) os_part="apple-darwin" ;;
        *) err "unsupported operating system: $os" ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch_part="x86_64" ;;
        arm64|aarch64) arch_part="aarch64" ;;
        *) err "unsupported architecture: $arch" ;;
    esac

    printf '%s-%s' "$arch_part" "$os_part"
}

TARGET=$(detect_target)
ASSET="${BIN}-${TARGET}.tar.gz"

info "Ratchet installer"
info "  platform : ${TARGET}"
info "  version  : ${VERSION}"

# ----- pick a download tool -----

if command -v curl >/dev/null 2>&1; then
    download() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    download() { wget -qO "$2" "$1"; }
else
    err "either curl or wget is required"
fi

# ----- resolve install directory -----

if [ -n "${RATCHET_INSTALL_DIR:-}" ]; then
    INSTALL_DIR="$RATCHET_INSTALL_DIR"
elif [ -w /usr/local/bin ] 2>/dev/null; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
fi

# ----- download -----

if [ "$VERSION" = "latest" ]; then
    BASE_URL="https://github.com/${REPO}/releases/latest/download"
else
    BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
fi

TMP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t ratchet)
cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT INT TERM

info "  downloading ${ASSET}"

if ! download "${BASE_URL}/${ASSET}" "${TMP_DIR}/${ASSET}"; then
    err "could not download ${BASE_URL}/${ASSET}

No release may exist yet for this platform, or the tag is wrong.
Build from source instead:

    git clone https://github.com/${REPO}
    cd ratchet-harness && cargo build --release"
fi

# ----- verify checksum (best effort) -----

if download "${BASE_URL}/SHA256SUMS" "${TMP_DIR}/SHA256SUMS" 2>/dev/null; then
    expected=$(grep " ${ASSET}\$" "${TMP_DIR}/SHA256SUMS" 2>/dev/null | awk '{print $1}' || true)
    if [ -n "$expected" ]; then
        if command -v sha256sum >/dev/null 2>&1; then
            actual=$(sha256sum "${TMP_DIR}/${ASSET}" | awk '{print $1}')
        elif command -v shasum >/dev/null 2>&1; then
            actual=$(shasum -a 256 "${TMP_DIR}/${ASSET}" | awk '{print $1}')
        else
            actual=""
            info "  checksum : skipped (no sha256sum or shasum available)"
        fi

        if [ -n "$actual" ]; then
            if [ "$actual" != "$expected" ]; then
                err "checksum mismatch for ${ASSET}
  expected: ${expected}
  actual:   ${actual}"
            fi
            info "  checksum : verified"
        fi
    fi
else
    info "  checksum : not published for this release"
fi

# ----- extract -----

tar -xzf "${TMP_DIR}/${ASSET}" -C "$TMP_DIR" || err "could not extract ${ASSET}"

SRC_BIN="${TMP_DIR}/${BIN}-${TARGET}/${BIN}"
[ -f "$SRC_BIN" ] || SRC_BIN=$(find "$TMP_DIR" -type f -name "$BIN" | head -n 1)
[ -n "$SRC_BIN" ] && [ -f "$SRC_BIN" ] || err "binary not found inside ${ASSET}"

# ----- install -----

mkdir -p "$INSTALL_DIR" || err "could not create ${INSTALL_DIR}"
install -m 755 "$SRC_BIN" "${INSTALL_DIR}/${BIN}" 2>/dev/null \
    || { cp "$SRC_BIN" "${INSTALL_DIR}/${BIN}" && chmod 755 "${INSTALL_DIR}/${BIN}"; }

info ""
info "Installed ${BIN} to ${INSTALL_DIR}/${BIN}"

# ----- PATH hint -----

case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
        info ""
        info "${INSTALL_DIR} is not on your PATH. Add it:"
        info ""
        info "  # bash"
        info "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.bashrc"
        info ""
        info "  # zsh"
        info "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.zshrc"
        ;;
esac

if "${INSTALL_DIR}/${BIN}" --version >/dev/null 2>&1; then
    info ""
    info "Run '${BIN} --help' to get started."
fi
