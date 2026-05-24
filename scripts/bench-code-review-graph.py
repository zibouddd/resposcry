#!/usr/bin/env python3
"""Benchmark RepoScry against code-review-graph on the same repository.

This runner is intentionally conservative:
- it records failures instead of hiding them;
- it does not require code-review-graph unless --require-crg is passed;
- it cleans only generated graph caches by default.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


GENERATED_CACHE_DIRS = [
    ".reposcry",
    ".code-review-graph",
    ".code_review_graph",
    ".crg",
    ".crg_cache",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare RepoScry and code-review-graph indexing latency."
    )
    parser.add_argument("--repo", default=".", help="Repository to benchmark.")
    parser.add_argument(
        "--out",
        default="benchmarks/out/latest-code-review-graph-compare.json",
        help="JSON output path.",
    )
    parser.add_argument(
        "--reposcry-bin",
        default=None,
        help="Path to reposcry. Defaults to target/release/reposcry(.exe).",
    )
    parser.add_argument(
        "--reposcry-update-bin",
        default=None,
        help="Path to reposcry-update. Defaults to target/release/reposcry-update(.exe).",
    )
    parser.add_argument(
        "--crg-bin",
        default=os.environ.get("CRG_BIN", "code-review-graph"),
        help="code-review-graph executable.",
    )
    parser.add_argument(
        "--skip-cargo-build",
        action="store_true",
        help="Do not build RepoScry release binaries before benchmarking.",
    )
    parser.add_argument(
        "--no-clean",
        action="store_true",
        help="Do not delete generated graph caches before cold runs.",
    )
    parser.add_argument(
        "--require-crg",
        action="store_true",
        help="Fail the runner when code-review-graph is unavailable or fails.",
    )
    return parser.parse_args()


def exe_suffix() -> str:
    return ".exe" if os.name == "nt" else ""


def default_release_bin(root: Path, name: str) -> Path:
    return root / "target" / "release" / f"{name}{exe_suffix()}"


def command_exists(command: str, env: dict[str, str] | None = None) -> bool:
    search_path = None if env is None else env.get("PATH")
    return shutil.which(command, path=search_path) is not None or Path(command).exists()


def run_timed(
    name: str,
    command: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    allow_failure: bool = False,
) -> dict[str, Any]:
    started = time.perf_counter()
    proc = subprocess.run(
        command,
        cwd=str(cwd),
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    elapsed_ms = int((time.perf_counter() - started) * 1000)
    result: dict[str, Any] = {
        "name": name,
        "command": command,
        "ok": proc.returncode == 0,
        "exit_code": proc.returncode,
        "elapsed_ms": elapsed_ms,
        "stdout_tail": proc.stdout[-4000:],
        "stderr_tail": proc.stderr[-4000:],
    }
    if proc.returncode != 0 and not allow_failure:
        raise RuntimeError(
            f"{name} failed with exit code {proc.returncode}: {proc.stderr[-1000:]}"
        )
    return result


def clean_generated_caches(repo: Path) -> None:
    for relative in GENERATED_CACHE_DIRS:
        path = repo / relative
        if path.exists():
            if path.is_dir():
                shutil.rmtree(path)
            else:
                path.unlink()


def ensure_release_binaries(root: Path, skip_cargo_build: bool) -> None:
    if skip_cargo_build:
        return
    subprocess.run(
        ["cargo", "build", "--release", "-p", "reposcry-cli", "--bins"],
        cwd=str(root),
        check=True,
    )


def command_env_with_repo_bins(reposcry_bin: Path) -> dict[str, str]:
    env = os.environ.copy()
    bin_dir = str(reposcry_bin.parent)
    env["PATH"] = bin_dir + os.pathsep + env.get("PATH", "")
    return env


def file_count(repo: Path) -> int:
    ignored_parts = {".git", "target", "node_modules", ".reposcry", ".code-review-graph"}
    count = 0
    for path in repo.rglob("*"):
        if not path.is_file():
            continue
        if any(part in ignored_parts for part in path.parts):
            continue
        count += 1
    return count


def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parents[1]
    repo = Path(args.repo).resolve()
    out_path = Path(args.out)
    if not out_path.is_absolute():
        out_path = root / out_path

    if not repo.exists():
        raise SystemExit(f"Repository does not exist: {repo}")

    reposcry_bin = (
        Path(args.reposcry_bin).resolve()
        if args.reposcry_bin
        else default_release_bin(root, "reposcry")
    )
    reposcry_update_bin = (
        Path(args.reposcry_update_bin).resolve()
        if args.reposcry_update_bin
        else default_release_bin(root, "reposcry-update")
    )

    ensure_release_binaries(root, args.skip_cargo_build)

    if not reposcry_bin.exists():
        raise SystemExit(f"reposcry binary not found: {reposcry_bin}")
    if not reposcry_update_bin.exists():
        raise SystemExit(f"reposcry-update binary not found: {reposcry_update_bin}")

    command_env = command_env_with_repo_bins(reposcry_bin)

    if not args.no_clean:
        clean_generated_caches(repo)

    results: list[dict[str, Any]] = []
    results.append(
        run_timed(
            "reposcry_cold_index_no_semantic",
            [str(reposcry_bin), "--repo", str(repo), "index", "--no-semantic"],
            cwd=repo,
            env=command_env,
        )
    )
    results.append(
        run_timed(
            "reposcry_warm_index_no_semantic",
            [str(reposcry_bin), "--repo", str(repo), "index", "--no-semantic"],
            cwd=repo,
            env=command_env,
        )
    )
    results.append(
        run_timed(
            "reposcry_incremental_readme_refresh_search",
            [
                str(reposcry_update_bin),
                "--repo",
                str(repo),
                "--file",
                "README.md",
                "--refresh-search",
                "--skip-warm-calls",
            ],
            cwd=repo,
            env=command_env,
        )
    )

    crg_available = command_exists(args.crg_bin, command_env)
    if crg_available:
        if not args.no_clean:
            for relative in [".code-review-graph", ".code_review_graph", ".crg", ".crg_cache"]:
                path = repo / relative
                if path.exists():
                    shutil.rmtree(path) if path.is_dir() else path.unlink()
        try:
            results.append(
                run_timed(
                    "code_review_graph_build",
                    [args.crg_bin, "build"],
                    cwd=repo,
                    env=command_env,
                    allow_failure=not args.require_crg,
                )
            )
        except RuntimeError:
            if args.require_crg:
                raise
    else:
        missing = {
            "name": "code_review_graph_build",
            "command": [args.crg_bin, "build"],
            "ok": False,
            "exit_code": None,
            "elapsed_ms": None,
            "stdout_tail": "",
            "stderr_tail": "code-review-graph executable was not found. Install with `pipx install code-review-graph` or pass --crg-bin.",
        }
        results.append(missing)
        if args.require_crg:
            raise SystemExit(missing["stderr_tail"])

    payload = {
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "repo": {
            "path": str(repo),
            "file_count": file_count(repo),
        },
        "machine": {
            "os": platform.platform(),
            "python": platform.python_version(),
            "cpu": platform.processor() or platform.machine(),
        },
        "tools": {
            "reposcry_bin": str(reposcry_bin),
            "reposcry_update_bin": str(reposcry_update_bin),
            "code_review_graph_bin": args.crg_bin,
            "code_review_graph_available": crg_available,
        },
        "results": results,
    }

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(payload, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
