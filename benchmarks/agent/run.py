"""Agent-level benchmark runner.

Compares three modes across tasks defined in tasks.json:

  A. opencode baseline       (grep/read only)
  B. opencode + CRG          (code-review-graph first)
  C. opencode + RepoScry     (reposcry first)

Usage:
  # Score reposcry context output for all tasks
  python benchmarks/agent/run.py --mode reposcry

  # Score all modes (requires code-review-graph and a running LLM harness)
  python benchmarks/agent/run.py --mode all

  # Score a single task
  python benchmarks/agent/run.py --mode reposcry --task nav_request_handling
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import time
import uuid
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TASKS_FILE = ROOT / "benchmarks" / "agent" / "tasks.json"
OUT_DIR = ROOT / "benchmarks" / "out"
AGENT_DIR = ROOT / "benchmarks" / "agent"

os.makedirs(OUT_DIR, exist_ok=True)


# ─── Scoring helpers ───────────────────────────────────────────────────────

def count_tokens(text: str) -> int:
    return max(1, len(text) // 4)


def load_tasks() -> list[dict]:
    return json.loads(TASKS_FILE.read_text(encoding="utf-8"))


def score_task_output(text: str, task: dict, mode: str) -> dict:
    """Score a mode's output for a task. Returns {answer_score, ...}."""
    text_lower = text.lower()
    gt_files = [f.lower() for f in task.get("expected_files", [])]
    gt_terms = [t.lower() for t in task.get("expected_terms", [])]

    # File recall
    extracted_files = re.findall(r'^###\s+(\S+)', text, re.MULTILINE)
    if not extracted_files:
        extracted_files = re.findall(
            r'crates/\S+\.(?:rs|toml|json|yaml|yml|js|ts|py|sh|ps1)',
            text, re.IGNORECASE
        )
    unique_files = list(dict.fromkeys(extracted_files))
    matched_files = sum(1 for f in set(unique_files) if f.lower() in gt_files)
    file_recall = matched_files / len(gt_files) if gt_files else 0.0

    # Term recall (symbols, concepts)
    matched_terms = sum(1 for t in gt_terms if t in text_lower)
    term_recall = matched_terms / len(gt_terms) if gt_terms else 0.0

    # Noise: duplicate files
    dup_files = max(0, len(extracted_files) - len(unique_files))

    # Compactness (output uses budget proportionally)
    tokens = count_tokens(text)
    compactness = 1.0 if tokens < 8000 else (0.5 if tokens < 16000 else 0.0)

    # Base score (0-5)
    base = (2.0 * file_recall + 2.0 * term_recall + 0.5 * compactness)
    noise_penalty = min(0.75, dup_files * 0.15)
    answer_score = max(0.0, min(5.0, base - noise_penalty))

    return {
        "answer_score": round(answer_score, 2),
        "file_recall": round(file_recall, 3),
        "term_recall": round(term_recall, 3),
        "file_count": len(unique_files),
        "matched_files": matched_files,
        "matched_terms": matched_terms,
        "estimated_tokens": tokens,
        "dup_files": dup_files,
        "compactness": compactness,
    }


# ─── Mode runners ──────────────────────────────────────────────────────────

MODE_CONFIGS = {
    "reposcry": {
        "command": ["reposcry", "context", "{task}", "--strict", "--budget=20000", "--format=markdown"],
    },
    "reposcry_review": {
        "command": ["reposcry", "get_review_context", "{task}", "--strict", "--budget=20000", "--format=markdown"],
    },
}


def run_mode_reposcry(task: dict) -> dict:
    prompt = task["prompt"]
    cmd = [part.replace("{task}", prompt) for part in MODE_CONFIGS["reposcry"]["command"]]
    t0 = time.perf_counter()

    try:
        result = subprocess.run(
            cmd, cwd=str(ROOT), capture_output=True, text=True, timeout=120
        )
        elapsed_ms = int((time.perf_counter() - t0) * 1000)
        ok = result.returncode == 0
        output = result.stdout if ok else result.stderr
    except subprocess.TimeoutExpired:
        elapsed_ms = 120_000
        ok = False
        output = "[TIMEOUT]"
    except FileNotFoundError:
        return {"mode": "reposcry", "ok": False, "error": "reposcry not found on PATH",
                "elapsed_ms": 0, "output_text": "", "estimated_tokens": 0}

    score = score_task_output(output, task, "reposcry")
    return {
        "mode": "reposcry",
        "task_id": task["id"],
        "ok": ok,
        "elapsed_ms": elapsed_ms,
        "output_text": output[:5000],
        "estimated_tokens": score["estimated_tokens"],
        **score,
    }


def run_mode_reposcry_review(task: dict) -> dict:
    prompt = task["prompt"]
    cmd = [part.replace("{task}", prompt) for part in MODE_CONFIGS["reposcry_review"]["command"]]
    t0 = time.perf_counter()

    try:
        result = subprocess.run(
            cmd, cwd=str(ROOT), capture_output=True, text=True, timeout=120
        )
        elapsed_ms = int((time.perf_counter() - t0) * 1000)
        ok = result.returncode == 0
        output = result.stdout if ok else result.stderr
    except subprocess.TimeoutExpired:
        elapsed_ms = 120_000
        ok = False
        output = "[TIMEOUT]"
    except FileNotFoundError:
        return {"mode": "reposcry_review", "ok": False, "error": "reposcry not found on PATH",
                "elapsed_ms": 0, "output_text": "", "estimated_tokens": 0}

    score = score_task_output(output, task, "reposcry_review")
    return {
        "mode": "reposcry_review",
        "task_id": task["id"],
        "ok": ok,
        "elapsed_ms": elapsed_ms,
        "output_text": output[:5000],
        "estimated_tokens": score["estimated_tokens"],
        **score,
    }


# ─── Report ────────────────────────────────────────────────────────────────

def fmt(n: int | float) -> str:
    if isinstance(n, float):
        return f"{n:.2f}"
    if n >= 1000:
        return f"{n/1000:.1f}k"
    return str(n)


def print_results(results: list[dict]):
    if not results:
        print("  No results.")
        return

    by_mode: dict[str, list[dict]] = {}
    for r in results:
        by_mode.setdefault(r["mode"], []).append(r)

    for mode, mode_results in by_mode.items():
        scores = [r.get("answer_score", 0) for r in mode_results]
        tokens = [r.get("estimated_tokens", 0) for r in mode_results]
        files_found = [r.get("file_count", 0) for r in mode_results]
        avg_score = sum(scores) / len(scores) if scores else 0
        avg_tokens = sum(tokens) / len(tokens) if tokens else 0
        avg_files = sum(files_found) / len(files_found) if files_found else 0
        wall = sum(r.get("elapsed_ms", 0) for r in mode_results)
        pct_ok = sum(1 for r in mode_results if r.get("ok")) / len(mode_results) * 100

        print(f"\n  Mode: {mode} ({len(mode_results)} tasks)")
        print(f"  {'─' * 60}")
        print(f"  Avg score:      {avg_score:.2f} / 5")
        print(f"  Avg tokens:     {fmt(avg_tokens)}")
        print(f"  Avg files:      {avg_files:.1f}")
        print(f"  Total wall:     {wall}ms")
        print(f"  Success rate:   {pct_ok:.0f}%")
        print(f"  {'─' * 60}")
        print(f"  {'Task':<35} {'Score':>6} {'Tok':>6} {'Files':>6} {'Wall':>8}")
        print(f"  {'─' * 35} {'─' * 6} {'─' * 6} {'─' * 6} {'─' * 8}")
        for r in mode_results:
            print(f"  {r.get('task_id', '?'):<35} {r.get('answer_score', 0):>6.2f} "
                  f"{fmt(r.get('estimated_tokens', 0)):>6} {r.get('file_count', 0):>6} "
                  f"{r.get('elapsed_ms', 0):>8}")

    # Head-to-head averages
    print(f"\n  {'═' * 60}")
    print(f"  SUMMARY (averages across all tasks)")
    print(f"  {'═' * 60}")
    header = f"  {'Mode':<20} {'Score':>6} {'Tokens':>7} {'Files':>6} {'Time':>8} {'OK%':>5}"
    print(header)
    print(f"  {'─' * 20} {'─' * 6} {'─' * 7} {'─' * 6} {'─' * 8} {'─' * 5}")
    for mode, mode_results in sorted(by_mode.items()):
        scores = [r.get("answer_score", 0) for r in mode_results]
        tokens = [r.get("estimated_tokens", 0) for r in mode_results]
        files_found = [r.get("file_count", 0) for r in mode_results]
        wall = [r.get("elapsed_ms", 0) for r in mode_results]
        avg_score = sum(scores) / len(scores) if scores else 0
        avg_tokens = sum(tokens) / len(tokens) if tokens else 0
        avg_files = sum(files_found) / len(files_found) if files_found else 0
        total_wall = sum(wall)
        pct_ok = sum(1 for r in mode_results if r.get("ok")) / len(mode_results) * 100
        print(f"  {mode:<20} {avg_score:>6.2f} {fmt(avg_tokens):>7} {avg_files:>6.1f} "
              f"{total_wall:>8} {pct_ok:>5.0f}")


# ─── Main ──────────────────────────────────────────────────────────────────

def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(description="Agent-level benchmark runner")
    parser.add_argument("--mode", choices=["reposcry", "reposcry_review", "all"],
                        default="reposcry", help="Which mode to run")
    parser.add_argument("--task", type=str, default=None,
                        help="Single task ID to run (default: all)")
    parser.add_argument("--save", action="store_true", default=True,
                        help="Save results to benchmarks/out/")
    args = parser.parse_args()

    tasks = load_tasks()
    if args.task:
        tasks = [t for t in tasks if t["id"] == args.task]
        if not tasks:
            print(f"Task '{args.task}' not found in tasks.json")
            return 1

    print(f"  Agent benchmark: {len(tasks)} task(s)")
    print(f"  Mode: {args.mode}")
    print(f"  Repo: {ROOT.name}, commit: {_get_commit()}")
    print(f"  {'─' * 60}")

    all_results: list[dict] = []
    runners = []
    if args.mode in ("reposcry", "all"):
        runners.append(run_mode_reposcry)
    if args.mode in ("reposcry_review", "all"):
        runners.append(run_mode_reposcry_review)

    for task in tasks:
        print(f"\n  Task: {task['id']} ({task['type']})")
        for runner in runners:
            print(f"    Running {runner.__name__}... ", end="", flush=True)
            result = runner(task)
            print(f"{'OK' if result.get('ok') else 'FAIL'} "
                  f"(score={result.get('answer_score', 0):.2f}, "
                  f"tokens={result.get('estimated_tokens', 0)}, "
                  f"wall={result.get('elapsed_ms', 0)}ms)")
            all_results.append(result)

    # Print report
    print(f"\n\n  {'=' * 60}")
    print(f"  RESULTS")
    print(f"  {'=' * 60}")
    print_results(all_results)

    # Save
    if args.save:
        commit = _get_commit()
        ts = time.strftime("%Y%m%d_%H%M%S")
        report = {
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "repo": ROOT.name,
            "commit": commit,
            "mode": args.mode,
            "tasks": len(tasks),
            "results": all_results,
        }
        out_path = OUT_DIR / f"agent-bench-{args.mode}-{ts}.json"
        out_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"\n  Results saved to {out_path.relative_to(ROOT)}")

    return 0


def _get_commit() -> str:
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=str(ROOT), capture_output=True, text=True, timeout=10
        ).stdout.strip()[:12]
    except Exception:
        return "unknown"


if __name__ == "__main__":
    raise SystemExit(main())
