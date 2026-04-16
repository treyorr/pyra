#!/bin/sh

set -eu

# Install the latest compatible Pyra release archive from GitHub Releases.
# This script is designed to be hostable from pyra.dev or raw GitHub.

REPO="${PYRA_REPO:-treyorr/pyra}"
BINARY_NAME="pyra"
INSTALL_DIR="${PYRA_INSTALL_DIR:-}"
VERSION="${PYRA_VERSION:-}"

say() {
  printf '%s\n' "$*"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

make_temp_dir() {
  if tmp="$(mktemp -d 2>/dev/null)"; then
    printf '%s\n' "$tmp"
    return
  fi

  mktemp -d -t pyra-install
}

normalize_tag() {
  case "$1" in
    "") printf '%s\n' "" ;;
    v*) printf '%s\n' "$1" ;;
    *) printf 'v%s\n' "$1" ;;
  esac
}

current_target() {
  os="$(uname -s 2>/dev/null || printf 'unknown')"
  arch="$(uname -m 2>/dev/null || printf 'unknown')"

  case "$os:$arch" in
    Darwin:arm64 | Darwin:aarch64)
      printf '%s\n' "aarch64-apple-darwin"
      ;;
    Darwin:x86_64)
      printf '%s\n' "x86_64-apple-darwin"
      ;;
    Linux:x86_64 | Linux:amd64)
      printf '%s\n' "x86_64-unknown-linux-gnu"
      ;;
    *)
      die "no published Pyra release archive is available for $os/$arch"
      ;;
  esac
}

sha256_file() {
  file="$1"

  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
    return
  fi

  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
    return
  fi

  if command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$file" | awk '{print $NF}'
    return
  fi

  die "missing checksum tool: need sha256sum, shasum, or openssl"
}

choose_install_dir() {
  if [ -n "$INSTALL_DIR" ]; then
    printf '%s\n' "$INSTALL_DIR"
    return
  fi

  if [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
    printf '%s\n' "/usr/local/bin"
    return
  fi

  printf '%s\n' "${HOME}/.local/bin"
}

need_cmd curl
need_cmd tar

TARGET="$(current_target)"
ARCHIVE_NAME="${BINARY_NAME}-${TARGET}.tar.gz"
CHECKSUM_NAME="${ARCHIVE_NAME}.sha256"
TAG="$(normalize_tag "$VERSION")"

if [ -n "$TAG" ]; then
  BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"
  RELEASE_LABEL="$TAG"
else
  BASE_URL="https://github.com/${REPO}/releases/latest/download"
  RELEASE_LABEL="the latest release"
fi

TMP_DIR="$(make_temp_dir)"
ARCHIVE_PATH="${TMP_DIR}/${ARCHIVE_NAME}"
CHECKSUM_PATH="${TMP_DIR}/${CHECKSUM_NAME}"

cleanup() {
  rm -rf "$TMP_DIR"
}

trap cleanup EXIT INT TERM HUP

say "Installing Pyra from ${RELEASE_LABEL} for ${TARGET}..."

if ! curl -fsSL "${BASE_URL}/${ARCHIVE_NAME}" -o "$ARCHIVE_PATH"; then
  die "failed to download ${ARCHIVE_NAME}; if no tagged release exists yet, install from source with: cargo install --locked --git https://github.com/${REPO} pyra-cli --bin pyra"
fi

if ! curl -fsSL "${BASE_URL}/${CHECKSUM_NAME}" -o "$CHECKSUM_PATH"; then
  die "failed to download ${CHECKSUM_NAME}"
fi

EXPECTED_SUM="$(awk '{print $1}' "$CHECKSUM_PATH")"
ACTUAL_SUM="$(sha256_file "$ARCHIVE_PATH")"

[ "$EXPECTED_SUM" = "$ACTUAL_SUM" ] || die "checksum verification failed for ${ARCHIVE_NAME}"

tar -xzf "$ARCHIVE_PATH" -C "$TMP_DIR"
[ -f "${TMP_DIR}/${BINARY_NAME}" ] || die "archive did not contain ${BINARY_NAME}"

DEST_DIR="$(choose_install_dir)"
mkdir -p "$DEST_DIR"

if command -v install >/dev/null 2>&1; then
  install -m 0755 "${TMP_DIR}/${BINARY_NAME}" "${DEST_DIR}/${BINARY_NAME}"
else
  cp "${TMP_DIR}/${BINARY_NAME}" "${DEST_DIR}/${BINARY_NAME}"
  chmod 0755 "${DEST_DIR}/${BINARY_NAME}"
fi

say "Installed ${BINARY_NAME} to ${DEST_DIR}/${BINARY_NAME}"

case ":$PATH:" in
  *:"${DEST_DIR}":*)
    ;;
  *)
    say "Add ${DEST_DIR} to your PATH to run ${BINARY_NAME} from new shells."
    ;;
esac

say "Run 'pyra --version' to verify the installation."
