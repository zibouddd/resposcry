#!/usr/bin/env bash
set -euo pipefail
BASE="${1:-main}"
reposcry-watch --repo . --base "$BASE" --refresh-search --skip-warm-calls
