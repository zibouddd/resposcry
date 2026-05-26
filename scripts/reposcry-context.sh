#!/usr/bin/env bash
set -euo pipefail
TASK="${*:-Review the current change safely}"
mkdir -p .reposcry
reposcry --repo . index --no-semantic
reposcry --repo . context "$TASK" --strict --budget "${REPOSCRY_TOKEN_BUDGET:-20000}" --format markdown > .reposcry/AI_CONTEXT.md
printf 'Wrote .reposcry/AI_CONTEXT.md\n'
