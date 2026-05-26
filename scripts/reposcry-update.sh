#!/usr/bin/env bash
set -euo pipefail
BASE="${1:-main}"
reposcry-update --repo . --changed --base "$BASE"
