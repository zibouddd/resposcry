#!/usr/bin/env bash
set -euo pipefail

REPO="${REPOSCRY_REPO:-zibouddd/resposcry}"
INSTALL_DIR="${REPOSCRY_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${REPOSCRY_VERSION:-latest}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

uname_s="${REPOSCRY_OS:-$(uname -s)}"
uname_m="${REPOSCRY_ARCH:-$(uname -m)}"

case "$uname_s" in
  Linux) os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *)
    echo "Unsupported OS: $uname_s" >&2
    exit 1
    ;;
esac

case "$uname_m" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *)
    echo "Unsupported architecture: $uname_m" >&2
    exit 1
    ;;
esac

asset="reposcry-${arch}-${os}.tar.gz"
checksum_asset="${asset}.sha256"

if [[ -n "${REPOSCRY_RELEASE_BASE_URL:-}" ]]; then
  base_url="${REPOSCRY_RELEASE_BASE_URL%/}"
elif [[ "$VERSION" == "latest" ]]; then
  base_url="https://github.com/${REPO}/releases/latest/download"
else
  base_url="https://github.com/${REPO}/releases/download/${VERSION}"
fi

url="${base_url}/${asset}"
checksum_url="${base_url}/${checksum_asset}"

verify_checksum() {
  local checksum_file="$1"
  local asset_file="$2"

  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$(dirname "$asset_file")" && sha256sum -c "$(basename "$checksum_file")")
  elif command -v shasum >/dev/null 2>&1; then
    local expected actual
    expected="$(awk '{print $1}' "$checksum_file")"
    actual="$(shasum -a 256 "$asset_file" | awk '{print $1}')"
    [[ "$expected" == "$actual" ]]
  else
    echo "No SHA-256 verification tool found (expected sha256sum or shasum)." >&2
    exit 1
  fi
}

mkdir -p "$INSTALL_DIR"
curl -fsSL "$url" -o "$TMP_DIR/$asset"
curl -fsSL "$checksum_url" -o "$TMP_DIR/$checksum_asset"
verify_checksum "$TMP_DIR/$checksum_asset" "$TMP_DIR/$asset"
tar -xzf "$TMP_DIR/$asset" -C "$TMP_DIR"
install -m 0755 "$TMP_DIR/reposcry" "$INSTALL_DIR/reposcry"

echo "Installed reposcry to $INSTALL_DIR/reposcry"
