#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

"$ROOT_DIR/scripts/setup-benchmark-fixtures.sh" "small_rust_repo"

TMP_ROOT="$(mktemp -d)"
INSTALL_ROOT="$TMP_ROOT/install-root"
FIXTURE_REPO="$ROOT_DIR/benchmarks/fixtures/small_rust_repo"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

cargo install --path crates/reposcry-cli --force --offline --root "$INSTALL_ROOT" >/dev/null
REPOSCRY_BIN="$INSTALL_ROOT/bin/reposcry"

if [[ ! -x "$REPOSCRY_BIN" ]]; then
  echo "Installed reposcry binary was not found" >&2
  exit 1
fi

"$REPOSCRY_BIN" --repo "$FIXTURE_REPO" init >/dev/null
"$REPOSCRY_BIN" --repo "$FIXTURE_REPO" index --no-semantic >/dev/null
"$REPOSCRY_BIN" --repo "$FIXTURE_REPO" warm-calls >/dev/null
"$REPOSCRY_BIN" --repo "$FIXTURE_REPO" refresh-search --semantic-backend local-hash-v1 >/dev/null

ARCH_JSON="$("$REPOSCRY_BIN" --repo "$FIXTURE_REPO" get_architecture_overview --format json)"
QUERY_JSON="$("$REPOSCRY_BIN" --repo "$FIXTURE_REPO" query_graph "callers_of rebuild_graph" --no-runtime-calls)"
SEARCH_JSON="$("$REPOSCRY_BIN" --repo "$FIXTURE_REPO" semantic_search_nodes "graph cache rebuild" --limit 5 --semantic --semantic-backend local-hash-v1)"
MCP_JSON="$(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0.0.0"}}}' | "$REPOSCRY_BIN" mcp --repo "$FIXTURE_REPO")"

python3 - <<PY
import json

arch = json.loads("""$ARCH_JSON""")
query = json.loads("""$QUERY_JSON""")
search = json.loads("""$SEARCH_JSON""")
mcp = json.loads("""$MCP_JSON""")

assert arch["files_indexed"] > 0, "README smoke: expected indexed files"
assert arch["persisted_symbol_call_edges"] > 0, "README smoke: expected persisted symbol call edges"
assert len(query["edges"]) > 0, "README smoke: expected callers_of rebuild_graph edges"
assert len(search["hits"]) > 0, "README smoke: expected semantic search results"
assert mcp["result"]["serverInfo"]["name"] == "reposcry", "README smoke: unexpected MCP server name"
PY

"$REPOSCRY_BIN" --repo "$FIXTURE_REPO" validate main >/dev/null
echo "README smoke passed"
