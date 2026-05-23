#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST_PATH="$ROOT_DIR/benchmarks/fixtures.json"

python - <<PY | while IFS= read -r fixture; do
import json
from pathlib import Path

manifest = json.loads(Path(r"$MANIFEST_PATH").read_text(encoding="utf-8"))
for fixture in manifest["fixtures"]:
    print(fixture["name"])
PY
  if [[ "$fixture" == "current_repo" ]]; then
    unset REPOSCRY_BENCH_FIXTURE
  else
    export REPOSCRY_BENCH_FIXTURE="$fixture"
  fi
  unset BENCH_OUT_NAME
  "$ROOT_DIR/scripts/bench.sh"
done
