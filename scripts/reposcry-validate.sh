#!/usr/bin/env bash
set -euo pipefail
BASE="${1:-main}"
reposcry --repo . detect_changes "$BASE" HEAD --format json
reposcry --repo . validate "$BASE" HEAD
