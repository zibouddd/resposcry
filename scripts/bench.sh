#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MANIFEST_PATH="$ROOT_DIR/benchmarks/fixtures.json"
FIXTURE_NAME="${REPOSCRY_BENCH_FIXTURE:-current_repo}"

resolve_fixture_field() {
  local field="$1"
  python - <<PY
import json
from pathlib import Path

manifest = json.loads(Path(r"$MANIFEST_PATH").read_text(encoding="utf-8"))
fixture_name = "$FIXTURE_NAME"
for fixture in manifest["fixtures"]:
    if fixture["name"] == fixture_name:
        value = fixture.get("$field", "")
        if value is None:
            value = ""
        print(value)
        break
else:
    raise SystemExit(f"Unknown benchmark fixture: {fixture_name}")
PY
}

FIXTURE_PATH_REL="$(resolve_fixture_field path)"
FIXTURE_SIZE="$(resolve_fixture_field size)"
FIXTURE_NOTES="$(resolve_fixture_field notes)"
CALLERS_QUERY="$(resolve_fixture_field callers_query)"
SEMANTIC_QUERY="$(resolve_fixture_field semantic_query)"
FIXTURE_PATH="$(cd "$ROOT_DIR/$FIXTURE_PATH_REL" && pwd)"

if [[ "$FIXTURE_NAME" != "current_repo" ]]; then
  "$ROOT_DIR/scripts/setup-benchmark-fixtures.sh" "$FIXTURE_NAME"
fi

OUT_DIR="${BENCH_OUT_DIR:-benchmarks/out}"
if [[ -n "${BENCH_OUT_NAME:-}" ]]; then
  OUT_FILE="${OUT_DIR}/${BENCH_OUT_NAME}"
elif [[ -n "${REPOSCRY_BENCH_FIXTURE:-}" && -n "${REPOSCRY_BENCH_SEMANTIC_BACKEND:-}" ]]; then
  OUT_FILE="${OUT_DIR}/latest-${FIXTURE_NAME}-${REPOSCRY_BENCH_SEMANTIC_BACKEND}.json"
elif [[ -n "${REPOSCRY_BENCH_FIXTURE:-}" ]]; then
  OUT_FILE="${OUT_DIR}/latest-${FIXTURE_NAME}.json"
elif [[ -n "${REPOSCRY_BENCH_SEMANTIC_BACKEND:-}" ]]; then
  OUT_FILE="${OUT_DIR}/latest-${REPOSCRY_BENCH_SEMANTIC_BACKEND}.json"
else
  OUT_FILE="${OUT_DIR}/latest.json"
fi
mkdir -p "$OUT_DIR"

cargo build -p reposcry-cli --bins >/dev/null

REPOSCRY_BIN="${REPOSCRY_BIN:-$ROOT_DIR/target/debug/reposcry}"
SEMANTIC_BENCH_BACKEND="${REPOSCRY_BENCH_SEMANTIC_BACKEND:-}"

measure_ms() {
  local start end
  start=$(python - <<'PY'
import time
print(int(time.time() * 1000))
PY
)
  "$@"
  end=$(python - <<'PY'
import time
print(int(time.time() * 1000))
PY
)
  echo $((end - start))
}

run_json_command() {
  local tmp
  tmp="$(mktemp)"
  "$@" > "$tmp"
  cat "$tmp"
  rm -f "$tmp"
}

cold_index_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" index --no-semantic)
warm_index_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" index --no-semantic)
semantic_index_reuse_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" refresh-search --semantic-backend local-hash-v1)
semantic_index_reembed_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" refresh-search --semantic-backend local-hash-v1 --reembed-all)
call_warmup_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" warm-calls)
arch_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" get_architecture_overview --format json)
detect_changes_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" detect_changes main HEAD --format json)
affected_flows_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" get_affected_flows main HEAD --format json)
callers_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" query_graph "$CALLERS_QUERY")
search_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" semantic_search_nodes "$SEMANTIC_QUERY" --limit 20 --semantic --semantic-backend local-hash-v1)
custom_semantic_reuse_ms=""
custom_semantic_reembed_ms=""
if [[ -n "$SEMANTIC_BENCH_BACKEND" ]]; then
  custom_semantic_reuse_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" refresh-search --semantic-backend "$SEMANTIC_BENCH_BACKEND")
  custom_semantic_reembed_ms=$(measure_ms "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" refresh-search --semantic-backend "$SEMANTIC_BENCH_BACKEND" --reembed-all)
fi

arch_json="$(run_json_command "$REPOSCRY_BIN" --repo "$FIXTURE_PATH" get_architecture_overview --format json)"
db_size_bytes=0
if [[ -f "$FIXTURE_PATH/.reposcry/reposcry.db" ]]; then
  db_size_bytes=$(wc -c < "$FIXTURE_PATH/.reposcry/reposcry.db")
fi

python - <<PY > "$OUT_FILE"
import json
from datetime import datetime, timezone
import os
import platform

arch = json.loads("""$arch_json""")

result = {
    "captured_at": datetime.now(timezone.utc).isoformat(),
    "machine": {
        "os": platform.platform(),
        "cpu": platform.processor() or platform.machine(),
        "memory_gb": os.environ.get("REPOSCRY_MEMORY_GB", "unknown"),
    },
    "repo": {
        "path": "$FIXTURE_PATH",
        "fixture": "$FIXTURE_NAME",
        "size": "$FIXTURE_SIZE",
        "notes": "$FIXTURE_NOTES",
        "fixture_manifest": "benchmarks/fixtures.json",
    },
    "metrics": {
        "cold_index_ms": $cold_index_ms,
        "warm_index_ms": $warm_index_ms,
        "semantic_index_reuse_ms": $semantic_index_reuse_ms,
        "semantic_index_reembed_ms": $semantic_index_reembed_ms,
        "call_warmup_ms": $call_warmup_ms,
        "architecture_overview_ms": $arch_ms,
        "detect_changes_ms": $detect_changes_ms,
        "affected_flows_ms": $affected_flows_ms,
        "query_graph_callers_ms": $callers_ms,
        "semantic_search_ms": $search_ms,
        "db_size_bytes": $db_size_bytes,
        "files_indexed": arch.get("files_indexed", 0),
        "symbols_indexed": arch.get("symbols_indexed", 0),
        "imports_indexed": arch.get("imports_indexed", 0),
        "persisted_call_sites": arch.get("persisted_call_sites", 0),
        "persisted_symbol_call_edges": arch.get("persisted_symbol_call_edges", 0),
        "persisted_file_call_edges": arch.get("persisted_file_call_edges", 0),
        "total_edges": arch.get("resolved_import_edges", 0),
    },
}

if "$SEMANTIC_BENCH_BACKEND":
    result["metrics"]["semantic_index_backend"] = "$SEMANTIC_BENCH_BACKEND"
    result["metrics"]["semantic_index_backend_reuse_ms"] = int("$custom_semantic_reuse_ms")
    result["metrics"]["semantic_index_backend_reembed_ms"] = int("$custom_semantic_reembed_ms")

print(json.dumps(result, indent=2))
PY

cat "$OUT_FILE"
