"""Score RepoScry vs code-review-graph benchmark outputs with sub-metrics.

This post-processes ``benchmarks/out/fair-comparison.json`` and writes a stricter
score file that accounts for correctness, compactness, duplicate/noise penalties,
encoding problems, and test recommendation quality.

Usage:
    python benchmarks/score_v2.py
    python benchmarks/score_v2.py benchmarks/out/fair-comparison.json --check
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_INPUT = ROOT / "benchmarks" / "out" / "fair-comparison.json"
DEFAULT_OUTPUT = ROOT / "benchmarks" / "out" / "fair-comparison-scored-v2.json"
DEFAULT_SUMMARY = ROOT / "benchmarks" / "SCORING_V2_SUMMARY.md"

EXPECTED_FILES = [
    "crates/reposcry-cache/src/db.rs",
    "crates/reposcry-cache/src/lib.rs",
    "crates/reposcry-cache/Cargo.toml",
]

OPTIONAL_FILES = [
    "crates/reposcry-cache/src/file_hasher.rs",
    "crates/reposcry-context/src/lib.rs",
    "crates/reposcry-cli/src/main.rs",
    "crates/reposcry-cli/src/crg_cli.rs",
    "crates/reposcry-cli/src/bin/reposcry-update.rs",
]

EXPECTED_SYMBOLS = [
    "CacheDb",
    "CacheDb::open",
    "CacheDb::open_in_memory",
    "CacheDb::initialize",
    "CacheDb::set_config",
    "CacheDb::get_config",
    "CacheDb::insert_search_document",
    "CacheDb::insert_search_vector",
    "CacheDb::get_search_vectors",
    "CacheDb::clear_search_vectors",
]

EXPECTED_TESTS = [
    "cargo test -p reposcry-cache",
]

IRRELEVANT_RULE_PATTERNS = [
    "ui components should not directly query database",
    "filter state should be serializable",
    "frontend empty-state",
]

ENCODING_ARTIFACTS = [
    "â€”",
    "â€“",
    "â†’",
    "â†",
    "âœ“",
    "âœ—",
    "�",
]


def normalize_path(value: str) -> str:
    return value.replace("\\", "/").lower()


def normalize_text(value: str) -> str:
    return normalize_path(value).replace("—", "-").replace("–", "-").replace("→", "->")


def clamp(value: float, low: float = 0.0, high: float = 5.0) -> float:
    return max(low, min(high, value))


def count_tokens(text: str) -> int:
    return max(1, len(text) // 4)


def mode_text(mode: dict[str, Any]) -> str:
    pieces: list[str] = []
    if isinstance(mode.get("output_text"), str):
        pieces.append(mode["output_text"])
    for key in ("context", "result", "output"):
        if key in mode:
            pieces.append(json.dumps(mode[key], sort_keys=True, ensure_ascii=False))
    return "\n".join(pieces)


def context_object(mode: dict[str, Any]) -> dict[str, Any] | None:
    """Return the embedded RepoScry context object when present."""
    if isinstance(mode.get("context"), dict):
        return mode["context"]
    text = mode.get("output_text")
    if not isinstance(text, str):
        return None
    # review_context markdown wraps JSON in a code fence. Try to recover it.
    match = re.search(r"```json\s*(\{.*?\})\s*```", text, flags=re.DOTALL)
    if not match:
        return None
    try:
        parsed = json.loads(match.group(1))
    except json.JSONDecodeError:
        return None
    ctx = parsed.get("context")
    return ctx if isinstance(ctx, dict) else None


def extract_paths_from_context(ctx: dict[str, Any] | None) -> list[str]:
    if not ctx:
        return []
    files = ctx.get("relevant_files", [])
    paths: list[str] = []
    if isinstance(files, list):
        for file in files:
            if isinstance(file, dict) and isinstance(file.get("path"), str):
                paths.append(file["path"])
    return paths


def extract_paths_from_markdown(text: str) -> list[str]:
    paths = re.findall(r"^###\s+([^\n]+)$", text, flags=re.MULTILINE)
    if paths:
        return paths
    return re.findall(r"(?:crates|benchmarks|scripts|src)/[^\s`\]\)]+", text)


def extract_symbol_texts(ctx: dict[str, Any] | None, text: str) -> list[str]:
    symbols: list[str] = []
    if ctx:
        for file in ctx.get("relevant_files", []) or []:
            if isinstance(file, dict):
                for sym in file.get("important_symbols", []) or []:
                    if isinstance(sym, str):
                        symbols.append(sym)
    if symbols:
        return symbols
    for line in text.splitlines():
        if line.strip().startswith("-") and "::" in line:
            symbols.append(line.strip("- "))
    return symbols


def file_recall(text: str, ctx: dict[str, Any] | None) -> float:
    normalized = normalize_text(text)
    paths = {normalize_path(p) for p in extract_paths_from_context(ctx)}
    paths.update(normalize_path(p) for p in extract_paths_from_markdown(text))
    hits = 0
    for expected in EXPECTED_FILES:
        expected_norm = normalize_path(expected)
        if expected_norm in normalized or expected_norm in paths:
            hits += 1
    return hits / len(EXPECTED_FILES)


def symbol_recall(text: str, ctx: dict[str, Any] | None) -> float:
    normalized = normalize_text(text)
    symbol_text = normalize_text("\n".join(extract_symbol_texts(ctx, text)))
    haystack = normalized + "\n" + symbol_text
    hits = 0
    for symbol in EXPECTED_SYMBOLS:
        sym = normalize_text(symbol)
        bare = sym.split("::")[-1]
        if sym in haystack or bare in haystack:
            hits += 1
    return hits / len(EXPECTED_SYMBOLS)


def reverse_dep_quality(text: str, ctx: dict[str, Any] | None) -> float:
    normalized = normalize_text(text)
    if ctx and isinstance(ctx.get("reverse_dependencies"), list):
        rdeps = ctx["reverse_dependencies"]
        if rdeps:
            users = json.dumps(rdeps, sort_keys=True).lower()
            if "reposcry-cli" in users and "reposcry-context" in users:
                return 1.0
            return 0.6
    if "reverse depend" in normalized and "reposcry-cli" in normalized:
        return 0.8
    if "used by" in normalized:
        return 0.5
    return 0.0


def test_accuracy(text: str, ctx: dict[str, Any] | None) -> float:
    normalized = normalize_text(text)
    tests = []
    if ctx and isinstance(ctx.get("suggested_tests"), list):
        tests = [str(t) for t in ctx["suggested_tests"]]
    tests_text = normalize_text("\n".join(tests) + "\n" + text)
    if "cargo test -p reposcry-cache" in tests_text:
        return 1.0
    if "cargo test" in tests_text and "reposcry-cache" in tests_text:
        return 0.8
    if "cargo test" in tests_text:
        return 0.5
    if "suggested tests" in normalized or tests:
        return 0.25
    return 0.0


def actionability(text: str, ctx: dict[str, Any] | None) -> float:
    normalized = normalize_text(text)
    score = 0.0
    if "implementation plan" in normalized or (ctx and ctx.get("implementation_plan")):
        score += 0.35
    if "suggested files" in normalized or "read before editing" in normalized:
        score += 0.20
    if "risk warning" in normalized or (ctx and ctx.get("risk_warnings")):
        score += 0.15
    if "cachedb::open" in normalized or "open_in_memory" in normalized:
        score += 0.15
    if "cargo test -p reposcry-cache" in normalized:
        score += 0.15
    return min(score, 1.0)


def compactness(estimated_tokens: int) -> float:
    if estimated_tokens <= 3_000:
        return 1.0
    if estimated_tokens <= 4_000:
        return 0.9
    if estimated_tokens <= 8_000:
        return 0.75
    if estimated_tokens <= 16_000:
        return 0.45
    return 0.25


def duplicate_count(values: list[str]) -> int:
    normalized = [normalize_path(v.strip()) for v in values if v and v.strip()]
    return sum(count - 1 for count in Counter(normalized).values() if count > 1)


def dependency_paths(ctx: dict[str, Any] | None, text: str) -> list[str]:
    if ctx and isinstance(ctx.get("dependency_paths"), list):
        return [str(v) for v in ctx["dependency_paths"]]
    if "## Dependency paths" not in text:
        return []
    block = text.split("## Dependency paths", 1)[1].split("##", 1)[0]
    return [line.strip() for line in block.splitlines() if "->" in normalize_text(line) or "→" in line]


def reverse_dep_paths(ctx: dict[str, Any] | None, text: str) -> list[str]:
    if ctx and isinstance(ctx.get("reverse_dependencies"), list):
        return [str(v.get("path", "")) for v in ctx["reverse_dependencies"] if isinstance(v, dict)]
    return re.findall(r"^([^\n]+) is used by:", text, flags=re.MULTILINE)


def warning_lines(ctx: dict[str, Any] | None, text: str) -> list[str]:
    if ctx and isinstance(ctx.get("risk_warnings"), list):
        return [str(v) for v in ctx["risk_warnings"]]
    if "## Risk warnings" not in text:
        return []
    block = text.split("## Risk warnings", 1)[1].split("##", 1)[0]
    return [line.strip("- ") for line in block.splitlines() if line.strip().startswith("-")]


def encoding_error_count(text: str) -> int:
    return sum(text.count(artifact) for artifact in ENCODING_ARTIFACTS)


def irrelevant_rule_count(text: str, ctx: dict[str, Any] | None) -> int:
    rules_text = ""
    if ctx and isinstance(ctx.get("architecture_rules"), list):
        rules_text = "\n".join(str(v) for v in ctx["architecture_rules"])
    rules_text += "\n" + text
    normalized = normalize_text(rules_text)
    return sum(1 for pattern in IRRELEVANT_RULE_PATTERNS if pattern in normalized)


def irrelevant_test_count(text: str, ctx: dict[str, Any] | None) -> int:
    tests = []
    if ctx and isinstance(ctx.get("suggested_tests"), list):
        tests.extend(str(v) for v in ctx["suggested_tests"])
    tests.extend(re.findall(r"benchmarks/fixtures/[^\s`\]\)]+", normalize_path(text)))
    return len([t for t in tests if "benchmarks/fixtures" in normalize_path(t)])


def score_mode(mode: dict[str, Any]) -> dict[str, Any]:
    text = mode_text(mode)
    ctx = context_object(mode)
    tokens = int(mode.get("estimated_tokens") or count_tokens(text))

    paths = extract_paths_from_context(ctx) or extract_paths_from_markdown(text)
    symbols = extract_symbol_texts(ctx, text)
    deps = dependency_paths(ctx, text)
    rdeps = reverse_dep_paths(ctx, text)
    warnings = warning_lines(ctx, text)

    metrics = {
        "file_recall": round(file_recall(text, ctx), 3),
        "symbol_recall": round(symbol_recall(text, ctx), 3),
        "reverse_dep_quality": round(reverse_dep_quality(text, ctx), 3),
        "test_accuracy": round(test_accuracy(text, ctx), 3),
        "actionability": round(actionability(text, ctx), 3),
        "compactness": round(compactness(tokens), 3),
        "duplicate_path_count": duplicate_count(paths),
        "duplicate_symbol_count": duplicate_count(symbols),
        "duplicate_dep_path_count": duplicate_count(deps),
        "duplicate_reverse_dep_count": duplicate_count(rdeps),
        "duplicate_warning_count": duplicate_count(warnings),
        "encoding_error_count": encoding_error_count(text),
        "irrelevant_rule_count": irrelevant_rule_count(text, ctx),
        "irrelevant_test_count": irrelevant_test_count(text, ctx),
        "includes_expected_test": "cargo test -p reposcry-cache" in normalize_text(text),
        "includes_manifest": normalize_path("crates/reposcry-cache/Cargo.toml") in normalize_text(text),
    }

    base_score = (
        1.25 * metrics["file_recall"]
        + 1.25 * metrics["symbol_recall"]
        + 0.75 * metrics["reverse_dep_quality"]
        + 0.75 * metrics["test_accuracy"]
        + 0.75 * metrics["actionability"]
        + 0.25 * metrics["compactness"]
    )
    noise_penalty = (
        min(0.75, metrics["duplicate_path_count"] * 0.15)
        + min(0.50, metrics["duplicate_symbol_count"] * 0.05)
        + min(0.50, metrics["duplicate_dep_path_count"] * 0.10)
        + min(0.50, metrics["duplicate_reverse_dep_count"] * 0.10)
        + min(0.50, metrics["duplicate_warning_count"] * 0.10)
        + min(1.00, metrics["encoding_error_count"] * 0.10)
        + min(0.75, metrics["irrelevant_rule_count"] * 0.25)
        + min(0.75, metrics["irrelevant_test_count"] * 0.25)
    )
    final_score = clamp(base_score - noise_penalty)

    scored = dict(mode)
    scored.update(metrics)
    scored["quality_score"] = round(base_score, 3)
    scored["noise_penalty"] = round(noise_penalty, 3)
    scored["final_score"] = round(final_score, 3)
    scored["score_per_1k_tokens_v2"] = round((final_score / tokens) * 1000, 3) if tokens else 0.0
    scored["legacy_score"] = scored.get("score")
    return scored


def load_results(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def write_summary(scored: dict[str, Any], path: Path) -> None:
    modes = scored.get("modes", [])
    rows = sorted(
        modes,
        key=lambda m: (m.get("score_per_1k_tokens_v2") or 0.0),
        reverse=True,
    )

    lines = [
        "# Scoring v2 summary",
        "",
        f"Task: `{scored.get('task', '')}`",
        "",
        "| Mode | Tokens | Legacy | Final | Score/1k v2 | File recall | Symbol recall | Tests | Noise |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for row in rows:
        lines.append(
            "| {mode} | {tokens} | {legacy} | {final} | {spk} | {fr} | {sr} | {ta} | {noise} |".format(
                mode=row.get("mode", ""),
                tokens=row.get("estimated_tokens", ""),
                legacy=row.get("legacy_score", row.get("score", "")),
                final=row.get("final_score", ""),
                spk=row.get("score_per_1k_tokens_v2", ""),
                fr=row.get("file_recall", ""),
                sr=row.get("symbol_recall", ""),
                ta=row.get("test_accuracy", ""),
                noise=row.get("noise_penalty", ""),
            )
        )

    lines.extend(
        [
            "",
            "## Acceptance gates",
            "",
            "The compact RepoScry mode should pass:",
            "",
            "```text",
            "reposcry_context_budget_4000 final_score >= 4.5",
            "reposcry_context_budget_4000 score_per_1k_tokens_v2 > best CRG score_per_1k_tokens_v2",
            "duplicate_path_count == 0",
            "encoding_error_count == 0",
            "irrelevant_rule_count == 0",
            "includes_manifest == true",
            "includes_expected_test == true",
            "```",
            "",
        ]
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")


def check_gates(scored: dict[str, Any]) -> int:
    modes = scored.get("modes", [])
    target = next((m for m in modes if m.get("mode") == "reposcry_context_budget_4000"), None)
    if not target:
        print("missing reposcry_context_budget_4000", file=sys.stderr)
        return 2

    crg_best = max(
        (
            float(m.get("score_per_1k_tokens_v2") or 0.0)
            for m in modes
            if str(m.get("mode", "")).startswith("crg_")
        ),
        default=0.0,
    )
    failures = []
    if float(target.get("final_score") or 0.0) < 4.5:
        failures.append("final_score < 4.5")
    if float(target.get("score_per_1k_tokens_v2") or 0.0) <= crg_best:
        failures.append("score_per_1k_tokens_v2 <= best CRG")
    for key in ("duplicate_path_count", "encoding_error_count", "irrelevant_rule_count"):
        if int(target.get(key) or 0) != 0:
            failures.append(f"{key} != 0")
    if not target.get("includes_manifest"):
        failures.append("missing crates/reposcry-cache/Cargo.toml")
    if not target.get("includes_expected_test"):
        failures.append("missing cargo test -p reposcry-cache")

    if failures:
        print("Scoring v2 gates failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", nargs="?", default=str(DEFAULT_INPUT))
    parser.add_argument("--output", default=str(DEFAULT_OUTPUT))
    parser.add_argument("--summary", default=str(DEFAULT_SUMMARY))
    parser.add_argument("--check", action="store_true", help="Fail if compact RepoScry gates do not pass")
    args = parser.parse_args(argv)

    input_path = Path(args.input)
    output_path = Path(args.output)
    summary_path = Path(args.summary)

    data = load_results(input_path)
    scored = dict(data)
    scored["scoring_version"] = "v2"
    scored["expected_files"] = EXPECTED_FILES
    scored["optional_files"] = OPTIONAL_FILES
    scored["expected_symbols"] = EXPECTED_SYMBOLS
    scored["expected_tests"] = EXPECTED_TESTS
    scored["modes"] = [score_mode(mode) for mode in data.get("modes", [])]

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(scored, indent=2, ensure_ascii=False), encoding="utf-8")
    write_summary(scored, summary_path)

    print(f"Wrote {output_path}")
    print(f"Wrote {summary_path}")

    if args.check:
        return check_gates(scored)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
