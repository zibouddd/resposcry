#!/usr/bin/env bash
set -euo pipefail
BASE="${1:-main...HEAD}"
crg validate "$BASE"
