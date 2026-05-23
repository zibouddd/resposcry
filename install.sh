#!/usr/bin/env bash
set -euo pipefail

REPO="${REPOSCRY_REPO:-zibouddd/resposcry}"
INSTALL_DIR="${REPOSCRY_INSTALL_DIR:-$HOME/.local/bin}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

uname_s="$(uname -s)"
uname_m="$(uname -m)"

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
url="https://github.com/${REPO}/releases/latest/download/${asset}"

mkdir -p "$INSTALL_DIR"
curl -fsSL "$url" -o "$TMP_DIR/$asset"
tar -xzf "$TMP_DIR/$asset" -C "$TMP_DIR"
install -m 0755 "$TMP_DIR/reposcry" "$INSTALL_DIR/reposcry"

echo "Installed reposcry to $INSTALL_DIR/reposcry"
