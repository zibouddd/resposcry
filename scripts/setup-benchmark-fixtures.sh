#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST_PATH="$ROOT_DIR/benchmarks/fixtures.json"
TARGET_FIXTURE="${1:-}"

python - <<PY
import json
import subprocess
from pathlib import Path

root = Path(r"$ROOT_DIR")
target = "$TARGET_FIXTURE"
manifest = json.loads(Path(r"$MANIFEST_PATH").read_text(encoding="utf-8"))

for fixture in manifest["fixtures"]:
    if fixture["name"] == "current_repo":
        continue
    if target and fixture["name"] != target:
        continue

    repo_path = (root / fixture["path"]).resolve()
    if not (repo_path / ".git").exists():
        subprocess.run(["git", "-C", str(repo_path), "init"], check=True)
        subprocess.run(["git", "-C", str(repo_path), "checkout", "-B", "main"], check=True)
        subprocess.run(["git", "-C", str(repo_path), "config", "user.name", "RepoScry Fixtures"], check=True)
        subprocess.run(["git", "-C", str(repo_path), "config", "user.email", "fixtures@example.invalid"], check=True)
        subprocess.run(["git", "-C", str(repo_path), "add", "."], check=True)
        subprocess.run(["git", "-C", str(repo_path), "commit", "-m", "Initial fixture"], check=True)
        continue

    subprocess.run(["git", "-C", str(repo_path), "config", "user.name", "RepoScry Fixtures"], check=True)
    subprocess.run(["git", "-C", str(repo_path), "config", "user.email", "fixtures@example.invalid"], check=True)
    head = subprocess.run(
        ["git", "-C", str(repo_path), "rev-parse", "--verify", "HEAD"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if head.returncode != 0:
        subprocess.run(["git", "-C", str(repo_path), "checkout", "-B", "main"], check=True)
        subprocess.run(["git", "-C", str(repo_path), "add", "."], check=True)
        subprocess.run(["git", "-C", str(repo_path), "commit", "-m", "Initial fixture"], check=True)
PY
