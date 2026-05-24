#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_ROOT="$(mktemp -d)"
DIST_DIR="$TMP_ROOT/dist"
INSTALL_DIR="$TMP_ROOT/install-check"
BINS=(reposcry reposcry-update reposcry-watch reposcry-export reposcry-mcp-plus)

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

cargo build --release -p reposcry-cli --bins >/dev/null

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

for bin in "${BINS[@]}"; do
  release_bin="$ROOT_DIR/target/release/$bin"
  if [[ ! -x "$release_bin" ]]; then
    echo "Release binary was not produced at $release_bin" >&2
    exit 1
  fi
done

mkdir -p "$DIST_DIR"
for bin in "${BINS[@]}"; do
  cp "$ROOT_DIR/target/release/$bin" "$DIST_DIR/$bin"
done

ASSET="reposcry-${target}.tar.gz"
ASSET_PATH="$TMP_ROOT/$ASSET"
tar -czf "$ASSET_PATH" -C "$DIST_DIR" "${BINS[@]}"
shasum -a 256 "$ASSET_PATH" > "$ASSET_PATH.sha256"

export REPOSCRY_RELEASE_BASE_URL="file://$TMP_ROOT"
export REPOSCRY_INSTALL_DIR="$INSTALL_DIR"
bash ./install.sh >/dev/null

for bin in "${BINS[@]}"; do
  installed="$INSTALL_DIR/$bin"
  if [[ ! -x "$installed" ]]; then
    echo "Installed $bin was not found" >&2
    exit 1
  fi
  VERSION_OUTPUT="$("$installed" --version)"
  if [[ "$VERSION_OUTPUT" != "$bin"\ * ]]; then
    echo "Unexpected version output for $bin: $VERSION_OUTPUT" >&2
    exit 1
  fi
done

echo "Release smoke passed"
