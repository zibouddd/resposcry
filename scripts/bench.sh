#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${BENCH_OUT_DIR:-benchmarks/out}"
OUT_FILE="${OUT_DIR}/latest.json"
mkdir -p "$OUT_DIR"

cargo build -p reposcry-cli --bins >/dev/null

REPOSCRY_BIN="${REPOSCRY_BIN:-$ROOT_DIR/target/debug/reposcry}"

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

cold_index_ms=$(measure_ms "$REPOSCRY_BIN" --repo . index)
warm_index_ms=$(measure_ms "$REPOSCRY_BIN" --repo . index)
call_warmup_ms=$(measure_ms "$REPOSCRY_BIN" --repo . warm-calls)
arch_ms=$(measure_ms "$REPOSCRY_BIN" --repo . get_architecture_overview --format json)
detect_changes_ms=$(measure_ms "$REPOSCRY_BIN" --repo . detect_changes main HEAD --format json)
affected_flows_ms=$(measure_ms "$REPOSCRY_BIN" --repo . get_affected_flows main HEAD --format json)
callers_ms=$(measure_ms "$REPOSCRY_BIN" --repo . query_graph "callers_of rebuild_graph")
search_ms=$(measure_ms "$REPOSCRY_BIN" --repo . semantic_search_nodes "cache database calls" --limit 20 --semantic)

arch_json="$(run_json_command "$REPOSCRY_BIN" --repo . get_architecture_overview --format json)"
db_size_bytes=0
if [[ -f ".reposcry/reposcry.db" ]]; then
  db_size_bytes=$(wc -c < ".reposcry/reposcry.db")
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
        "path": "$ROOT_DIR",
        "fixture": "current_repo",
        "fixture_manifest": "benchmarks/fixtures.json",
    },
    "metrics": {
        "cold_index_ms": $cold_index_ms,
        "warm_index_ms": $warm_index_ms,
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

print(json.dumps(result, indent=2))
PY

cat "$OUT_FILE"
