#!/bin/sh
set -eu

# Installer for the herdr-direct fork build (IlyaMichlin/herdr).
# Downloads the latest `herdr-direct-*` release asset and installs it as `herdr`.
#
# Usage:
#   curl -fsSL https://github.com/IlyaMichlin/herdr/releases/latest/download/install-herdr-direct.sh | sh
#
# Override the release used:
#   HERDR_DIRECT_TAG=herdr-direct-v0.7.5 curl -fsSL ... | sh
# Override the install location (default: ~/.local/bin):
#   HERDR_INSTALL_DIR=/usr/local/bin curl -fsSL ... | sh

BIN="herdr"
REPO="IlyaMichlin/herdr"
INSTALL_DIR="${HERDR_INSTALL_DIR:-$HOME/.local/bin}"

main() {
    echo ""
    echo "      ,ww"
    echo "     wWWWWWWW_)  herdr-direct installer"
    echo "     \`WWWWWW'    github.com/${REPO}"
    echo "      II  II"
    echo ""

    # detect platform
    OS="$(uname -s)"
    case "$OS" in
        Linux)  os="linux" ;;
        Darwin) os="macos" ;;
        *)      err "unsupported OS: $OS" ;;
    esac

    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)  arch="aarch64" ;;
        *)              err "unsupported architecture: $ARCH" ;;
    esac

    log "detected ${os}/${arch}"

    # check dependencies
    need curl

    ASSET="herdr-direct-${os}-${arch}"

    # Pick the release: a pinned tag if provided, otherwise the latest release.
    if [ -n "${HERDR_DIRECT_TAG:-}" ]; then
        URL="https://github.com/${REPO}/releases/download/${HERDR_DIRECT_TAG}/${ASSET}"
        log "downloading ${ASSET} from ${HERDR_DIRECT_TAG}..."
    else
        URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
        log "downloading latest ${ASSET}..."
    fi

    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT

    if ! curl -fsSL --retry 3 --connect-timeout 10 --max-time 120 "$URL" -o "${TMP}/${BIN}"; then
        err "download failed from ${URL} — is there a published release with a ${ASSET} asset?"
    fi

    # install
    mkdir -p "$INSTALL_DIR"
    mv "${TMP}/${BIN}" "${INSTALL_DIR}/${BIN}"
    chmod +x "${INSTALL_DIR}/${BIN}"

    # curl downloads are not quarantined by Gatekeeper, but strip the attribute
    # defensively in case the binary was fetched some other way first.
    if [ "$os" = "macos" ] && command -v xattr >/dev/null 2>&1; then
        xattr -d com.apple.quarantine "${INSTALL_DIR}/${BIN}" 2>/dev/null || true
    fi

    log "installed ${BIN} to ${INSTALL_DIR}/${BIN}"

    # check PATH
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo ""
            warn "${INSTALL_DIR} is not in your PATH"
            echo "  add it to your shell config:"
            echo ""
            echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
            echo ""
            ;;
    esac

    # verify
    if command -v "$BIN" >/dev/null 2>&1; then
        echo ""
        log "ready. run 'herdr' to get started."
    fi

    echo ""
}

log()  { printf '  \033[32m>\033[0m %s\n' "$1"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$1"; }
err()  { printf '  \033[31m✗\033[0m %s\n' "$1" >&2; exit 1; }

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "requires '$1' — install it first, or download a binary manually from https://github.com/${REPO}/releases"
    fi
}

main "$@"
