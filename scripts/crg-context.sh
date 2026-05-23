#!/usr/bin/env bash
set -euo pipefail
TASK="${*:-Review the current change safely}"
mkdir -p .code-review-graph
crg index
crg context "$TASK" --strict --budget "${CRG_TOKEN_BUDGET:-20000}" --format markdown > .code-review-graph/AI_CONTEXT.md
printf 'Wrote .code-review-graph/AI_CONTEXT.md\n'
