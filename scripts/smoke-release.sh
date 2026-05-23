#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_ROOT="$(mktemp -d)"
DIST_DIR="$TMP_ROOT/dist"
INSTALL_DIR="$TMP_ROOT/install-check"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

cargo build --release -p reposcry-cli --bin reposcry >/dev/null

if [[ "$(uname -s)" == "Darwin" ]]; then
  case "$(uname -m)" in
    arm64|aarch64) target="aarch64-apple-darwin" ;;
    x86_64|amd64) target="x86_64-apple-darwin" ;;
    *) echo "Unsupported macOS architecture: $(uname -m)" >&2; exit 1 ;;
  esac
else
  case "$(uname -m)" in
    x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
    *) echo "Unsupported Linux architecture: $(uname -m)" >&2; exit 1 ;;
  esac
fi

RELEASE_BIN="$ROOT_DIR/target/release/reposcry"
if [[ ! -x "$RELEASE_BIN" ]]; then
  echo "Release binary was not produced at $RELEASE_BIN" >&2
  exit 1
fi

mkdir -p "$DIST_DIR"
cp "$RELEASE_BIN" "$DIST_DIR/reposcry"

ASSET="reposcry-${target}.tar.gz"
ASSET_PATH="$TMP_ROOT/$ASSET"
tar -czf "$ASSET_PATH" -C "$DIST_DIR" reposcry
sha256sum "$ASSET_PATH" > "$ASSET_PATH.sha256"

export REPOSCRY_RELEASE_BASE_URL="file://$TMP_ROOT"
export REPOSCRY_INSTALL_DIR="$INSTALL_DIR"
./install.sh >/dev/null

if [[ ! -x "$INSTALL_DIR/reposcry" ]]; then
  echo "Installed reposcry was not found" >&2
  exit 1
fi

VERSION_OUTPUT="$("$INSTALL_DIR/reposcry" --version)"
if [[ "$VERSION_OUTPUT" != reposcry\ * ]]; then
  echo "Unexpected version output: $VERSION_OUTPUT" >&2
  exit 1
fi

echo "Release smoke passed"
