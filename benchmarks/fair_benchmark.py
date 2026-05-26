"""Hardened benchmark: RepoScry vs CRG with scoring & budget sweep.

Compares multiple modes on both tools for the same task.
Scoring: 0=failed/empty 1=generic 2=partially relevant 3=mostly correct
         4=correct+actionable 5=correct+minimal+cites exact files/symbols
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRG_DB = ROOT / ".code-review-graph" / "graph.db"
WIKI_DIR = ROOT / ".code-review-graph" / "wiki"
TASK = "add a new cache backend to reposcry-cache crate"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def count_tokens(text: str) -> int:
    return max(1, len(text) // 4)


def count_words(text: str) -> int:
    return max(1, len(text.split()))


def fmt(n: int | float) -> str:
    if isinstance(n, float):
        if n >= 100:
            return f"{n:.0f}"
        return f"{n:.2f}"
    if n >= 1000:
        return f"{n/1000:.1f}k"
    return str(n)


def ensure_utf8(text: str) -> str:
    return text.encode("utf-8", errors="replace").decode("utf-8")


def read_all(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except Exception:
        return ""


# ---------------------------------------------------------------------------
# MODE 0: raw_repo
# ---------------------------------------------------------------------------

def mode_raw_repo() -> dict:
    included_exts = {".rs", ".ts", ".js", ".tsx", ".jsx", ".py",
                     ".toml", ".yaml", ".yml", ".json", ".md", ".sh", ".ps1"}
    excluded_dirs = {".git", "target", "node_modules", ".reposcry",
                     ".code-review-graph", "benchmarks/out"}
    total_bytes = 0
    total_lines = 0
    file_count = 0
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        if path.suffix not in included_exts:
            continue
        if any(part in excluded_dirs for part in path.parts):
            continue
        try:
            text = path.read_text(errors="replace")
            total_bytes += len(text.encode("utf-8"))
            total_lines += text.count("\n") + 1
            file_count += 1
        except (OSError, UnicodeDecodeError):
            pass

    return {
        "mode": "raw_repo",
        "bytes": total_bytes,
        "words": total_bytes // 5,
        "estimated_tokens": total_bytes // 4,
        "source_files": file_count,
        "source_lines": total_lines,
        "output_text": f"Full repository: {file_count} files, {total_lines} lines",
    }


# ---------------------------------------------------------------------------
# MODE 1: reposcry_context (sweeps budgets 4k, 8k, 16k, 20k)
# ---------------------------------------------------------------------------

def mode_reposcry_context(budget: int) -> dict:
    t0 = time.perf_counter()
    result = subprocess.run(
        ["reposcry", "context", TASK, "--strict",
         f"--budget={budget}", "--format=markdown"],
        cwd=str(ROOT), capture_output=True, text=True, timeout=120,
    )
    elapsed_ms = int((time.perf_counter() - t0) * 1000)
    out = result.stdout
    return {
        "mode": f"reposcry_context_budget_{budget}",
        "ok": result.returncode == 0,
        "elapsed_ms": elapsed_ms,
        "bytes": len(out.encode("utf-8")),
        "words": count_words(out),
        "estimated_tokens": count_tokens(out),
        "output_text": ensure_utf8(out),
        "stderr": result.stderr[:500] if result.returncode != 0 else "",
    }


# ---------------------------------------------------------------------------
# MODE 2: reposcry_review_context (sweeps budgets 4k, 8k, 16k, 20k)
# ---------------------------------------------------------------------------

def mode_reposcry_review_context(budget: int) -> dict:
    t0 = time.perf_counter()
    result = subprocess.run(
        ["reposcry", "get_review_context", TASK, "--strict",
         f"--budget={budget}", "--format=markdown"],
        cwd=str(ROOT), capture_output=True, text=True, timeout=120,
    )
    elapsed_ms = int((time.perf_counter() - t0) * 1000)
    out = result.stdout
    return {
        "mode": f"reposcry_review_context_budget_{budget}",
        "ok": result.returncode == 0,
        "elapsed_ms": elapsed_ms,
        "bytes": len(out.encode("utf-8")),
        "words": count_words(out),
        "estimated_tokens": count_tokens(out),
        "output_text": ensure_utf8(out),
        "stderr": result.stderr[:500] if result.returncode != 0 else "",
    }


# ---------------------------------------------------------------------------
# MODE 3: crg_mcp_raw — direct Python calls, may fail gracefully
# ---------------------------------------------------------------------------

def mode_crg_mcp_raw() -> dict:
    sys.path.insert(0, str(Path(sys.executable).parent / "Lib/site-packages"))
    results: list[dict] = []

    try:
        from code_review_graph.tools.context import get_minimal_context
        from code_review_graph.tools.query import (
            semantic_search_nodes, query_graph, list_graph_stats,
        )
    except ImportError as e:
        return {"mode": "crg_mcp_raw", "error": str(e), "ok": False,
                "output_text": "", "bytes": 0, "words": 0, "estimated_tokens": 0}

    repo_root = str(ROOT)

    calls = [
        ("minimal_context", lambda: get_minimal_context(task=TASK, repo_root=repo_root)),
        ("semantic_search_cache_backend", lambda: semantic_search_nodes(
            query="cache backend", limit=10, repo_root=repo_root)),
        ("semantic_search_cache_database", lambda: semantic_search_nodes(
            query="cache database", limit=10, repo_root=repo_root)),
        ("query_graph_CacheDb", lambda: query_graph(
            pattern="callers_of", target="CacheDb", repo_root=repo_root)),
        ("query_graph_importers_db_rs", lambda: query_graph(
            pattern="importers_of", target="crates/reposcry-cache/src/db.rs",
            repo_root=repo_root)),
        ("semantic_search_add_cache_backend", lambda: semantic_search_nodes(
            query="add cache backend to reposcry-cache", limit=15,
            repo_root=repo_root)),
        ("graph_stats", lambda: list_graph_stats(repo_root=repo_root)),
    ]

    combined_bytes = 0
    combined_tokens = 0
    combined_words = 0
    combined_lines: list[str] = []
    total_latency = 0

    for name, fn in calls:
        try:
            t0 = time.perf_counter()
            data = fn()
            elapsed = int((time.perf_counter() - t0) * 1000)
            text = json.dumps(data, indent=2)
            total_latency += elapsed
            combined_bytes += len(text.encode("utf-8"))
            combined_tokens += count_tokens(text)
            combined_words += count_words(text)
            summary = data.get("summary", json.dumps(data)[:200])
            combined_lines.append(f"=== {name} ({elapsed}ms) ===")
            combined_lines.append(summary)
            combined_lines.append("")
        except Exception as exc:
            combined_lines.append(f"=== {name} [FAIL] ===")
            combined_lines.append(str(exc))
            combined_lines.append("")

    return {
        "mode": "crg_mcp_raw",
        "ok": True,
        "elapsed_ms": total_latency,
        "bytes": combined_bytes,
        "words": combined_words,
        "estimated_tokens": combined_tokens,
        "call_count": len(calls),
        "output_text": "\n".join(combined_lines),
    }


# ---------------------------------------------------------------------------
# MODE 4: crg_mcp_retried — correct queries with absolute paths, FTS5
# ---------------------------------------------------------------------------

def mode_crg_mcp_retried() -> dict:
    sys.path.insert(0, str(Path(sys.executable).parent / "Lib/site-packages"))
    results: list[dict] = []

    try:
        from code_review_graph.tools.query import (
            semantic_search_nodes, query_graph, list_graph_stats,
        )
        from code_review_graph.tools._common import _get_store
    except ImportError as e:
        return {"mode": "crg_mcp_retried", "error": str(e), "ok": False,
                "output_text": "", "bytes": 0, "words": 0, "estimated_tokens": 0}

    repo_root = str(ROOT)
    abs_db_rs = str((ROOT / "crates/reposcry-cache/src/db.rs").resolve())
    abs_cache_dir = str((ROOT / "crates/reposcry-cache").resolve())

    combined_lines: list[str] = []
    combined_bytes = 0
    combined_tokens = 0
    combined_words = 0
    total_latency = 0

    # Helper to run and record
    def run(name: str, fn, is_json: bool = True):
        nonlocal combined_bytes, combined_tokens, combined_words, total_latency
        try:
            t0 = time.perf_counter()
            data = fn()
            elapsed = int((time.perf_counter() - t0) * 1000)
            text = json.dumps(data, indent=2) if is_json else str(data)
            total_latency += elapsed
            combined_bytes += len(text.encode("utf-8"))
            combined_tokens += count_tokens(text)
            combined_words += count_words(text)
            summary = data.get("summary", str(data)[:200]) if isinstance(data, dict) else str(data)[:200]
            combined_lines.append(f"=== {name} ({elapsed}ms) ===")
            combined_lines.append(summary)
            combined_lines.append("")
        except Exception as exc:
            combined_lines.append(f"=== {name} [FAIL] ===")
            combined_lines.append(str(exc))
            combined_lines.append("")

    # 1. Direct file query using absolute path
    try:
        store, root = _get_store(repo_root)
    except Exception as e:
        store = None

    if store:
        # 1a. All nodes in db.rs
        db_nodes = store.get_nodes_by_file(abs_db_rs)
        lines = [f"Nodes in {abs_db_rs}:"]
        for n in db_nodes:
            lines.append(f"  {n.kind:>10} {n.name:30} lines {n.line_start}-{n.line_end}")
        combined_lines.append("=== get_nodes_by_file (db.rs) ===")
        combined_lines.extend(lines)
        combined_lines.append("")
        text = "\n".join(lines)
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        # 1b. CALLS edges from db.rs file node
        file_qn = abs_db_rs.replace("\\", "\\\\")  # escape for SQL
        # Actually the file_node uses the raw path
        edges_out = store.get_edges_by_source(abs_db_rs)
        lines = ["CALLS edges from db.rs:"]
        for e in edges_out:
            if e.kind == "CALLS":
                src = e.source_qualified.split("::")[-1] if "::" in e.source_qualified else e.source_qualified
                tgt = e.target_qualified.split("::")[-1] if "::" in e.target_qualified else e.target_qualified
                lines.append(f"  {src} -> {tgt}")
        combined_lines.append("=== edges_by_source (db.rs, CALLS) ===")
        combined_lines.extend(lines[:30])
        if len(lines) > 30:
            combined_lines.append(f"  ... and {len(lines)-30} more")
        combined_lines.append("")
        text = "\n".join(lines[:32])
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        # 1c. IMPORTS_FROM edges targeting db.rs
        edges_in = store.get_edges_by_target(abs_db_rs)
        lines = ["IMPORTS_FROM edges targeting db.rs:"]
        for e in edges_in:
            if e.kind == "IMPORTS_FROM":
                src = e.source_qualified.split("::")[0] if "::" in e.source_qualified else e.source_qualified
                lines.append(f"  imported by: {src}")
        if len(lines) == 1:
            lines.append("  (none with full path — try bare name search)")

        # 1d. Also search by bare name for CALLS targets
        bare_edges = store.search_edges_by_target_name("CacheDb")
        if bare_edges:
            lines.append(f"\nBare-name CALLS to CacheDb ({len(bare_edges)} edges):")
            for e in bare_edges[:10]:
                src = e.source_qualified.split("::")[-1]
                lines.append(f"  {src} -> CacheDb")
        combined_lines.append("=== importers / callers of db.rs ===")
        combined_lines.extend(lines)
        combined_lines.append("")

        text = "\n".join(lines)
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        # 1e. All cached file nodes in the reposcry-cache directory
        all_files = store.get_all_files()
        cache_files = [f for f in all_files if "reposcry-cache" in f]
        lines = [f"Files in reposcry-cache crate:"]
        for f in cache_files:
            lines.append(f"  {f}")
        combined_lines.append("=== cache crate files ===")
        combined_lines.extend(lines)
        combined_lines.append("")
        text = "\n".join(lines)
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        store.close()

        # 1f. CONTAINS hierarchy for CacheDb class
        try:
            store, root = _get_store(repo_root)
            cache_db_qn = f"{abs_db_rs}::CacheDb"
            edges_contains = store.get_edges_by_source(cache_db_qn)
            lines = [f"CONTAINS edges for {cache_db_qn}:"]
            for e in edges_contains:
                if e.kind == "CONTAINS":
                    tgt = e.target_qualified.split("::")[-1]
                    lines.append(f"  contains: {tgt}")
            combined_lines.append("=== CacheDb class members ===")
            combined_lines.extend(lines)
            combined_lines.append("")
            text = "\n".join(lines)
            combined_bytes += len(text.encode("utf-8"))
            combined_tokens += count_tokens(text)
            combined_words += count_words(text)

            # 1g. FTS5 search for cache-related
            try:
                rows = store._conn.execute(
                    "SELECT n.name, n.kind, n.file_path FROM nodes_fts f "
                    "JOIN nodes n ON f.rowid = n.id "
                    "WHERE nodes_fts MATCH ? LIMIT 20",
                    ('"cache" OR "backend" OR "storage"',)
                ).fetchall()
                lines = ["FTS5 search results for 'cache' OR 'backend' OR 'storage':"]
                for r in rows:
                    lines.append(f"  {r[1]:>10} {r[0]:30} {r[2]}")
                combined_lines.append("=== FTS5 search ===")
                combined_lines.extend(lines)
                combined_lines.append("")
                text = "\n".join(lines)
                combined_bytes += len(text.encode("utf-8"))
                combined_tokens += count_tokens(text)
                combined_words += count_words(text)
            except Exception as e:
                pass

            store.close()
        except Exception as e:
            combined_lines.append(f"=== CacheDb class query [FAIL] ===")
            combined_lines.append(str(e))
            combined_lines.append("")

    # 2. graph_stats
    try:
        t0 = time.perf_counter()
        stats = list_graph_stats(repo_root=repo_root)
        elapsed = int((time.perf_counter() - t0) * 1000)
        text = json.dumps(stats, indent=2)
        total_latency += elapsed
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)
        combined_lines.append(f"=== graph_stats ({elapsed}ms) ===")
        combined_lines.append(stats.get("summary", ""))
        combined_lines.append("")
    except Exception as exc:
        combined_lines.append(f"=== graph_stats [FAIL] ===")
        combined_lines.append(str(exc))
        combined_lines.append("")

    return {
        "mode": "crg_mcp_retried",
        "ok": True,
        "elapsed_ms": total_latency,
        "bytes": combined_bytes,
        "words": combined_words,
        "estimated_tokens": combined_tokens,
        "output_text": "\n".join(combined_lines),
    }


# ---------------------------------------------------------------------------
# MODE 5: crg_wiki — all wiki page content combined
# ---------------------------------------------------------------------------

def mode_crg_wiki() -> dict:
    combined_bytes = 0
    combined_tokens = 0
    combined_words = 0
    combined_lines: list[str] = []

    if not WIKI_DIR.is_dir():
        return {"mode": "crg_wiki", "ok": False,
                "output_text": "No wiki directory found", "bytes": 0,
                "words": 0, "estimated_tokens": 0}

    pages = sorted(WIKI_DIR.glob("*.md"))
    for f in pages:
        text = f.read_text(errors="replace")
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)
        combined_lines.append(f"# {f.stem}")
        combined_lines.append(text)
        combined_lines.append("")

    return {
        "mode": "crg_wiki",
        "ok": True,
        "elapsed_ms": 0,
        "bytes": combined_bytes,
        "words": combined_words,
        "estimated_tokens": combined_tokens,
        "output_text": "\n".join(combined_lines),
        "call_count": len(pages),
    }


# ---------------------------------------------------------------------------
# MODE 6: crg_manual_best — best possible CRG context for the task
# ---------------------------------------------------------------------------

def mode_crg_manual_best() -> dict:
    """Manually construct best-possible CRG context using all available
    data: wiki, detect-changes CLI, and direct DB queries."""
    sys.path.insert(0, str(Path(sys.executable).parent / "Lib/site-packages"))
    combined_lines: list[str] = []
    combined_bytes = 0
    combined_tokens = 0
    combined_words = 0
    total_latency = 0

    abs_db_rs = str((ROOT / "crates/reposcry-cache/src/db.rs").resolve())
    abs_cache_dir = str((ROOT / "crates/reposcry-cache").resolve())

    # 1. Wiki pages for cache-related communities
    wiki_pages_text = ""
    if WIKI_DIR.is_dir():
        for f in sorted(WIKI_DIR.glob("*.md")):
            text = f.read_text(errors="replace")
            wiki_pages_text += f"# {f.stem}\n{text}\n\n"
    combined_lines.append("=== CRG Wiki Pages ===")
    combined_lines.append(wiki_pages_text[:8000])  # trim
    if len(wiki_pages_text) > 8000:
        combined_lines.append("... [wiki truncated]")

    text_batch = wiki_pages_text[:8000]
    combined_bytes += len(text_batch.encode("utf-8"))
    combined_tokens += count_tokens(text_batch)
    combined_words += count_words(text_batch)

    # 2. Direct DB queries for cache-specific info
    try:
        from code_review_graph.tools._common import _get_store
        store, root = _get_store(str(ROOT))

        # 2a. All nodes in cache crate
        cache_files = [f for f in store.get_all_files() if "reposcry-cache" in f]
        lines = ["\n=== Files in reposcry-cache crate ==="]
        for f in cache_files:
            rel = Path(f).relative_to(ROOT)
            count = len(store.get_nodes_by_file(f))
            lines.append(f"  {rel} ({count} symbols)")
        combined_lines.extend(lines)
        text = "\n".join(lines)
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        # 2b. Nodes in db.rs
        db_nodes = store.get_nodes_by_file(abs_db_rs)
        lines = ["\n=== Symbols in db.rs ==="]
        for n in db_nodes:
            lines.append(f"  {n.kind:>10} {n.name:35} L{n.line_start}-{n.line_end}")
        combined_lines.extend(lines)
        text = "\n".join(lines)
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        # 2c. Edges from db.rs (calls to other modules)
        lines = ["\n=== External calls from db.rs ==="]
        for e in store.get_edges_by_source(abs_db_rs):
            if e.kind == "CALLS":
                tgt = e.target_qualified.split("::")[-1]
                src = e.source_qualified.split("::")[-1]
                lines.append(f"  {src} -> {tgt}")
        combined_lines.extend(lines[:15])
        text = "\n".join(lines[:15])
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        # 2d. Bare name CALLS to CacheDb
        lines = ["\n=== Bare-name CALLS to CacheDb (callers) ==="]
        for e in store.search_edges_by_target_name("CacheDb"):
            src = e.source_qualified.split("::")[-1]
            fp = Path(e.file_path).relative_to(ROOT)
            lines.append(f"  {src:30} in {fp}")
        if len(lines) == 1:
            lines.append("  (none found by bare name)")
        combined_lines.extend(lines)
        text = "\n".join(lines)
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        # 2e. IMPORTS_FROM targeting cache crate files
        all_edges = []
        for f in cache_files:
            all_edges.extend(store.get_edges_by_target(f))
        lines = ["\n=== Files that import from cache crate ==="]
        seen_sources = set()
        for e in all_edges:
            if e.kind == "IMPORTS_FROM" and e.source_qualified not in seen_sources:
                seen_sources.add(e.source_qualified)
                src_file = e.source_qualified.split("::")[0]
                try:
                    rel = Path(src_file).relative_to(ROOT)
                except ValueError:
                    rel = src_file
                lines.append(f"  {rel}")
        if len(lines) == 1:
            lines.append("  (none — paths may be bare module names)")
        combined_lines.extend(lines[:15])
        text = "\n".join(lines[:15])
        combined_bytes += len(text.encode("utf-8"))
        combined_tokens += count_tokens(text)
        combined_words += count_words(text)

        # 2f. Flows that include cache files
        try:
            cursor = store._conn.execute(
                "SELECT f.name, f.criticality, f.node_count, f.file_count "
                "FROM flows f "
                "JOIN flow_memberships fm ON f.id = fm.flow_id "
                "JOIN nodes n ON fm.node_id = n.id "
                "WHERE n.file_path LIKE ? "
                "GROUP BY f.id ORDER BY f.criticality DESC LIMIT 5",
                ('%reposcry-cache%',)
            )
            rows = cursor.fetchall()
            lines = ["\n=== Flows involving cache crate ==="]
            for r in rows:
                lines.append(f"  {r[0]:30} criticality={r[1]:.2f} {r[2]} nodes in {r[3]} files")
            if len(lines) == 1:
                lines.append("  (none)")
            combined_lines.extend(lines)
            text = "\n".join(lines)
            combined_bytes += len(text.encode("utf-8"))
            combined_tokens += count_tokens(text)
            combined_words += count_words(text)
        except Exception:
            pass

        # 2g. Risk info for cache-related nodes
        try:
            cursor = store._conn.execute(
                "SELECT n.name, r.risk_score, r.caller_count, r.test_coverage "
                "FROM risk_index r JOIN nodes n ON r.node_id = n.id "
                "WHERE n.file_path LIKE ? "
                "ORDER BY r.risk_score DESC LIMIT 10",
                ('%reposcry-cache%',)
            )
            rows = cursor.fetchall()
            lines = ["\n=== Risk info for cache symbols ==="]
            for r in rows:
                lines.append(f"  {r[0]:30} risk={r[1]:.2f} callers={r[2]} tests={r[3]}")
            if len(lines) == 1:
                lines.append("  (none)")
            combined_lines.extend(lines)
            text = "\n".join(lines)
            combined_bytes += len(text.encode("utf-8"))
            combined_tokens += count_tokens(text)
            combined_words += count_words(text)
        except Exception:
            pass

        store.close()
    except Exception as exc:
        combined_lines.append(f"\n=== DB queries [FAIL] ===\n{exc}")

    # 3. detect-changes CLI output
    try:
        t0 = time.perf_counter()
        result = subprocess.run(
            ["code-review-graph", "detect-changes", "--brief"],
            cwd=str(ROOT), capture_output=True, text=True, timeout=30,
        )
        elapsed = int((time.perf_counter() - t0) * 1000)
        total_latency += elapsed
        out = result.stdout + result.stderr
        if out.strip():
            combined_lines.append(f"\n=== detect-changes CLI ({elapsed}ms) ===\n{out.strip()}")
            combined_bytes += len(out.encode("utf-8"))
            combined_tokens += count_tokens(out)
            combined_words += count_words(out)
    except Exception:
        pass

    return {
        "mode": "crg_manual_best",
        "ok": True,
        "elapsed_ms": total_latency,
        "bytes": combined_bytes,
        "words": combined_words,
        "estimated_tokens": combined_tokens,
        "output_text": "\n".join(combined_lines),
    }


# ---------------------------------------------------------------------------
# Ground truth
# ---------------------------------------------------------------------------

GROUND_TRUTH = {
    "expected_files": [
        "crates/reposcry-cache/src/db.rs",
        "crates/reposcry-cache/src/lib.rs",
        "crates/reposcry-cache/Cargo.toml",
    ],
    "optional_files": [
        "crates/reposcry-cache/src/file_hasher.rs",
        "crates/reposcry-context/src/lib.rs",
        "crates/reposcry-cli/src/main.rs",
        "crates/reposcry-cli/src/bin/reposcry-update.rs",
    ],
    "expected_symbols": [
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
    ],
    "expected_tests": [
        "cargo test -p reposcry-cache",
    ],
}


# ---------------------------------------------------------------------------
# Output analysis  (extract sub-metrics from each mode)
# ---------------------------------------------------------------------------

FILE_RE = re.compile(r'crates/\S+\.(?:rs|toml|json|yaml|yml)', re.IGNORECASE)
SYMBOL_RE = re.compile(r'(?:CacheDb|CachedFile|Symbol|Edge|DbIndex|SearchVector)'
                       r'(?:::\w+)*')
TEST_CMD_RE = re.compile(r'cargo\s+test\s+-p\s+\S+')
ENCODING_ERR_RE = re.compile(r'[\ufffd\ufffe]|\\\\x[eE][0-9a-fA-F]{2}|\\\\u[0-9a-fA-F]{4}')


def _parse_relevant_files(text: str) -> list[str]:
    """Extract file paths from RepoScry markdown '### path' lines."""
    return re.findall(r'^###\s+(crates/\S+)', text, re.MULTILINE)


def _parse_important_symbols(text: str) -> list[str]:
    """Extract symbols from RepoScry 'Important symbols:' section."""
    syms = []
    in_section = False
    for line in text.splitlines():
        if line.strip().startswith("Important symbols:"):
            in_section = True
            continue
        if in_section:
            if line.startswith("## ") or line.strip() == "":
                in_section = False
            elif line.strip().startswith("- "):
                syms.append(line.strip()[2:])
    return syms


def _parse_suggested_tests(text: str) -> list[str]:
    """Extract tests from '## Suggested tests' section."""
    tests = []
    in_section = False
    for line in text.splitlines():
        if line.strip() == "## Suggested tests":
            in_section = True
            continue
        if in_section:
            if line.startswith("## ") or line.strip() == "":
                in_section = False
            elif line.strip().startswith("- "):
                tests.append(line.strip()[2:])
    return tests


def _parse_dependency_paths(text: str) -> list[str]:
    deps = []
    in_section = False
    for line in text.splitlines():
        if line.strip() == "## Dependency paths":
            in_section = True
            continue
        if in_section:
            if line.startswith("## ") or line.strip() == "":
                in_section = False
            else:
                deps.append(line.strip())
    return deps


def _parse_reverse_deps(text: str) -> list[str]:
    rdeps = []
    in_section = False
    for line in text.splitlines():
        if line.strip() == "## Reverse dependencies":
            in_section = True
            continue
        if in_section:
            if line.startswith("## ") or line.strip() == "":
                in_section = False
            elif line.strip().startswith("- "):
                rdeps.append(line.strip()[2:])
    return rdeps


def _parse_architecture_rules(text: str) -> list[str]:
    rules = []
    in_section = False
    for line in text.splitlines():
        if line.strip() == "## Architecture rules":
            in_section = True
            continue
        if in_section:
            if line.startswith("## ") or line.strip() == "":
                in_section = False
            elif line.strip().startswith("- "):
                rules.append(line.strip()[2:])
    return rules


def _parse_risk_warnings(text: str) -> list[str]:
    warnings = []
    in_section = False
    for line in text.splitlines():
        if line.strip() == "## Risk warnings":
            in_section = True
            continue
        if in_section:
            if line.startswith("## ") or line.strip() == "":
                in_section = False
            elif line.strip().startswith("- "):
                warnings.append(line.strip()[2:])
    return warnings


def analyze_output(text: str, mode: str) -> dict:
    """Compute sub-metrics for scoring v2 from a mode's output text."""
    text_lower = text.lower()
    gt = GROUND_TRUTH

    # ── File extraction ──────────────────────────────────────────────
    extracted_files: list[str] = _parse_relevant_files(text)
    if not extracted_files:
        extracted_files = list(set(FILE_RE.findall(text)))

    # Deduplicate for recall calculation
    unique_files = list(dict.fromkeys(extracted_files))
    expected_set = set(gt["expected_files"])
    optional_set = set(gt["optional_files"])
    if expected_set:
        matched_expected = sum(1 for f in set(unique_files) if f in expected_set)
        file_recall = matched_expected / len(expected_set)
    else:
        file_recall = 0.0

    # ── Symbol extraction ────────────────────────────────────────────
    extracted_syms: list[str] = _parse_important_symbols(text)
    if not extracted_syms:
        extracted_syms = list(set(SYMBOL_RE.findall(text)))

    unique_syms = list(dict.fromkeys(extracted_syms))
    # Match ground truth symbols by any occurrence in text (case-insensitive)
    # Also match partial: e.g. "CacheDb::open" matches if "CacheDb" is found
    gt_sym_lower = [s.lower() for s in gt["expected_symbols"]]
    sym_recall = sum(
        1 for s in gt_sym_lower if s in text_lower
    ) / len(gt_sym_lower) if gt_sym_lower else 0.0

    # ── Reverse dep quality ──────────────────────────────────────────
    rdeps = _parse_reverse_deps(text)
    has_consumers = any(
        c in text_lower
        for c in ["reposcry-context", "reposcry-cli", "reposcry-export", "crg_cli.rs", "main.rs"]
    )
    has_imports = any(
        c in text_lower for c in ["imports", "uses", "calls", "depends on", "consumers"]
    )
    if has_consumers:
        reverse_dep_quality = 1.0
    elif has_imports or len(rdeps) > 0:
        reverse_dep_quality = 0.5
    else:
        reverse_dep_quality = 0.0

    # ── Test accuracy ────────────────────────────────────────────────
    # RepoScry suggests test FILE PATHS (e.g. crates/reposcry-cache/tests/), not cargo commands.
    # CRG outputs may also mention tests differently. Score via multiple signals:
    extracted_tests: list[str] = _parse_suggested_tests(text)
    if not extracted_tests:
        extracted_tests = list(set(TEST_CMD_RE.findall(text)))
    test_accuracy = 0.0
    # 1. Exact cargo test command match
    for t in gt["expected_tests"]:
        if t.lower() in text_lower:
            test_accuracy = 1.0
            break
    # 2. Test file paths in the output (RepoScry's "Suggested tests" section)
    if test_accuracy == 0.0:
        reposcry_cache_tests = [t for t in extracted_tests
                                if "reposcry-cache" in t and "test" in t.lower()]
        if reposcry_cache_tests:
            test_accuracy = 0.7
        elif any("reposcry-cache" in f and "test" in f.lower()
                 for f in extracted_files):
            test_accuracy = 0.5

    # ── Actionability ────────────────────────────────────────────────
    has_files = len(extracted_files) > 0
    has_syms = len(extracted_syms) > 0
    has_structure = len(rdeps) > 0 or "dependency" in text_lower
    if has_files and has_syms and has_structure:
        actionability = 1.0
    elif has_files and has_syms:
        actionability = 0.5
    else:
        actionability = 0.0

    # ── Compactness ──────────────────────────────────────────────────
    tokens = len(text) // 4
    budget_strs = re.findall(r'budget[=_](\d+)', mode)
    budget = int(budget_strs[0]) if budget_strs else 4000
    ratio = tokens / budget if budget > 0 else 1.0
    if ratio < 0.75:
        compactness = 1.0
    elif ratio < 1.0:
        compactness = 0.5
    else:
        compactness = 0.0

    # ── Noise: duplicates (count redundant occurrences beyond first) ─
    from collections import Counter
    path_counts = Counter(extracted_files)
    dup_paths = max(0, len(extracted_files) - len(unique_files))

    sym_counts = Counter(extracted_syms)
    dup_syms = max(0, len(extracted_syms) - len(unique_syms))

    dep_paths_list = _parse_dependency_paths(text)
    dup_deps = max(0, len(dep_paths_list) - len(set(dep_paths_list)))

    rdep_items = _parse_reverse_deps(text)
    dup_rdeps = max(0, len(rdep_items) - len(set(rdep_items)))

    warnings_list = _parse_risk_warnings(text)
    dup_warnings = max(0, len(warnings_list) - len(set(warnings_list)))

    # ── Encoding errors ──────────────────────────────────────────────
    encoding_errors = len(ENCODING_ERR_RE.findall(text))

    # ── Irrelevant rules ─────────────────────────────────────────────
    rules = _parse_architecture_rules(text)
    irrelevant_rules = 0
    for rule in rules:
        rule_lower = rule.lower()
        mentioned_crate = None
        for crate in ["reposcry-cache", "reposcry-cli", "reposcry-context", "reposcry-graph"]:
            if crate in rule_lower:
                mentioned_crate = crate
                break
        if mentioned_crate:
            crate_in_files = any(mentioned_crate in f for f in extracted_files)
            if not crate_in_files:
                irrelevant_rules += 1

    # ── Irrelevant tests ─────────────────────────────────────────────
    irrelevant_tests = 0
    for test in extracted_tests:
        if "reposcry-cache" not in test.lower():
            irrelevant_tests += 1

    return {
        "file_recall": round(file_recall, 3),
        "symbol_recall": round(sym_recall, 3),
        "reverse_dep_quality": reverse_dep_quality,
        "test_accuracy": round(test_accuracy, 3),
        "actionability": actionability,
        "compactness": compactness,
        # raw counts for noise calculation
        "duplicate_path_count": dup_paths,
        "duplicate_symbol_count": dup_syms,
        "duplicate_dep_path_count": dup_deps,
        "duplicate_reverse_dep_count": dup_rdeps,
        "duplicate_warning_count": dup_warnings,
        "encoding_error_count": encoding_errors,
        "irrelevant_rule_count": irrelevant_rules,
        "irrelevant_test_count": irrelevant_tests,
        # extracted items for debug
        "extracted_files": extracted_files,
        "extracted_symbols": extracted_syms,
        "extracted_tests": extracted_tests,
    }


# ---------------------------------------------------------------------------
# Scoring v2 formula
# ---------------------------------------------------------------------------

def score_v2(analysis: dict, tokens: int, mode: str) -> dict:
    a = analysis

    base_score = (
        1.25 * a["file_recall"]
        + 1.25 * a["symbol_recall"]
        + 0.75 * a["reverse_dep_quality"]
        + 0.75 * a["test_accuracy"]
        + 0.75 * a["actionability"]
        + 0.25 * a["compactness"]
    )

    noise_penalty = (
        min(0.75, a["duplicate_path_count"] * 0.15)
        + min(0.50, a["duplicate_symbol_count"] * 0.05)
        + min(0.50, a["duplicate_dep_path_count"] * 0.10)
        + min(0.50, a["duplicate_reverse_dep_count"] * 0.10)
        + min(0.50, a["duplicate_warning_count"] * 0.10)
        + min(1.00, a["encoding_error_count"] * 0.10)
        + min(0.75, a["irrelevant_rule_count"] * 0.25)
        + min(0.75, a["irrelevant_test_count"] * 0.25)
    )

    final_score = max(0.0, min(5.0, base_score - noise_penalty))
    sc_per_k = round(final_score / max(1, tokens) * 1000, 3)

    return {
        "quality_score": round(base_score, 3),
        "noise_penalty": round(noise_penalty, 3),
        "final_score": round(final_score, 3),
        "score_per_1k_tokens": sc_per_k,
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> int:
    print("[1/7] Raw repo baseline...")
    raw = mode_raw_repo()

    print("[2/7] CRG MCP raw...")
    crg_raw = mode_crg_mcp_raw()

    print("[3/7] CRG MCP retried...")
    crg_retried = mode_crg_mcp_retried()

    print("[4/7] CRG wiki...")
    crg_wiki = mode_crg_wiki()

    print("[5/7] CRG manual best...")
    crg_best = mode_crg_manual_best()

    print("[6/7] RepoScry (sweeping budgets)...")
    reposcry_results = []
    for budget in [4000, 8000, 16000, 20000]:
        print(f"  context budget={budget}...")
        reposcry_results.append(mode_reposcry_context(budget))
        print(f"  review_context budget={budget}...")
        reposcry_results.append(mode_reposcry_review_context(budget))

    print("[7/7] Scoring and reporting...")
    all_modes = [
        crg_raw, crg_retried, crg_wiki, crg_best,
    ] + reposcry_results

    # Score each mode with v2 scoring
    for m in all_modes:
        text = m.get("output_text", "")
        analysis = analyze_output(text, m.get("mode", ""))
        m["analysis"] = analysis
        v2 = score_v2(analysis, m.get("estimated_tokens", 1), m.get("mode", ""))
        m["quality_score"] = v2["quality_score"]
        m["noise_penalty"] = v2["noise_penalty"]
        m["score"] = v2["final_score"]
        m["score_per_1k_tokens"] = v2["score_per_1k_tokens"]

    # Build report
    print("=" * 78)
    print(f"  SCORING v2: RepoScry vs CRG")
    print(f"  Task: {TASK}")
    print(f"  Repo: {raw['source_files']} files, {raw['source_lines']} lines, ~{fmt(raw['estimated_tokens'])} tokens")
    print("=" * 78)

    # Main detailed table
    header = f"  {'Mode':<40} {'Tok':>5} {'Lat':>6} {'FileR':>6} {'SymR':>6} {'TestA':>6} {'Noise':>6} {'Score':>5} {'Sc/1k':>6}"
    sep = '-' * len(header)
    print(f"\n{sep}")
    print(header)
    print(sep)

    printed_modes: set[str] = set()
    for m in all_modes:
        mode = m["mode"]
        if mode in printed_modes:
            continue
        printed_modes.add(mode)
        ok = m.get("ok", True) and "error" not in m
        if not ok:
            print(f"  {'[FAIL] ' + mode:<40} {'--':>5} {'--':>6} {'--':>6} {'--':>6} {'--':>6} {'--':>6} {'0':>5} {'0':>6}")
            continue
        a = m.get("analysis", {})
        print(f"  {mode:<40} {fmt(m['estimated_tokens']):>5} {fmt(m.get('elapsed_ms',0))+'ms':>6} "
              f"{a.get('file_recall',0):>6.2f} {a.get('symbol_recall',0):>6.2f} "
              f"{a.get('test_accuracy',0):>6.2f} {m['noise_penalty']:>6.3f} "
              f"{m['score']:>5.2f} {m['score_per_1k_tokens']:>6.3f}")

    # Summary ranking (by score per 1k)
    print(f"\n{'='*78}")
    print(f"  RANKING by Score / 1k tokens")
    print(f"{'='*78}")
    scored = [m for m in all_modes if m.get("ok", True) and "error" not in m]
    scored.sort(key=lambda m: m.get("score_per_1k_tokens", 0), reverse=True)
    print(f"  {'Mode':<40} {'Score':>6} {'Tok':>7} {'Sc/1k':>7} {'Base':>7} {'Noise':>7} {'FileR':>6} {'SymR':>6}")
    print(f"  {'-'*40} {'-'*6} {'-'*7} {'-'*7} {'-'*7} {'-'*7} {'-'*6} {'-'*6}")
    for m in scored:
        a = m.get("analysis", {})
        print(f"  {m['mode']:<40} {m['score']:>6.2f} {fmt(m['estimated_tokens']):>7} "
              f"{m['score_per_1k_tokens']:>7.3f} {m['quality_score']:>7.3f} "
              f"{m['noise_penalty']:>7.3f} {a.get('file_recall',0):>6.2f} {a.get('symbol_recall',0):>6.2f}")

    # Budget sweep
    print(f"\n{'='*78}")
    print(f"  REPOSCRY BUDGET SWEEP")
    print(f"{'='*78}")
    print(f"  {'Budget':>8} {'Mode':<28} {'Tok':>5} {'Score':>5} {'Noise':>6} {'Sc/1k':>6} {'FileR':>6} {'SymR':>6} {'Lat':>6}")
    print(f"  {'-'*8} {'-'*28} {'-'*5} {'-'*5} {'-'*6} {'-'*6} {'-'*6} {'-'*6} {'-'*6}")
    for m in reposcry_results:
        budget_str = m["mode"].split("_")[-1]
        mode_short = "context" if "context_budget" in m["mode"] and "review" not in m["mode"] else \
                     "review_context" if "review_context_budget" in m["mode"] else m["mode"]
        a = m.get("analysis", {})
        print(f"  {budget_str:>8} {mode_short:<28} {fmt(m['estimated_tokens']):>5} "
              f"{m['score']:>5.2f} {m['noise_penalty']:>6.3f} "
              f"{m['score_per_1k_tokens']:>6.3f} {a.get('file_recall',0):>6.2f} "
              f"{a.get('symbol_recall',0):>6.2f} {fmt(m.get('elapsed_ms',0))+'ms':>6}")

    # Head-to-head
    print(f"\n{'='*78}")
    print(f"  HEAD-TO-HEAD: BEST SCORE PER TOOL")
    print(f"{'='*78}")

    best_reposcry = max(reposcry_results, key=lambda m: m["score_per_1k_tokens"])
    crg_modes = [crg_raw, crg_retried, crg_wiki, crg_best]
    best_crg = max(crg_modes, key=lambda m: m.get("score_per_1k_tokens", 0))

    for label, m in [("RepoScry (best)", best_reposcry), ("CRG (best)", best_crg)]:
        a = m.get("analysis", {})
        print(f"  {label+':':<20} Score={m['score']:.2f}  Sc/1k={m['score_per_1k_tokens']:.3f}  "
              f"Tok={fmt(m['estimated_tokens'])}  FileR={a.get('file_recall',0):.2f}  "
              f"SymR={a.get('symbol_recall',0):.2f}  Noise={m['noise_penalty']:.3f}")

    # Keep the old score for continuity
    print(f"\n  OLD score (0-5 heuristic):")
    from collections import Counter as _C
    _SIGNALS = ["reposcry-cache", "db.rs", "CacheDb", "CachedFile", "edge.rs",
                "symbol.rs", "crg_cli.rs", "main.rs", "reposcry-export.rs",
                "reposcry-update.rs", "reposcry-context", "reposcry-mcp-plus"]
    for m in [best_reposcry, best_crg]:
        text_lower = m.get("output_text", "").lower()
        old_score = sum(1 for s in _SIGNALS if s.lower() in text_lower)
        print(f"  {m['mode']:<45} {old_score}/{len(_SIGNALS)} signals matched")

    # Save JSON
    out_path = ROOT / "benchmarks" / "out" / "fair-comparison.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "task": TASK,
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "scoring_version": "v2",
        "ground_truth": GROUND_TRUTH,
        "raw_source": raw,
        "modes": [{
            "mode": m["mode"],
            "estimated_tokens": m["estimated_tokens"],
            "elapsed_ms": m.get("elapsed_ms", 0),
            "analysis": m.get("analysis", {}),
            "quality_score": m.get("quality_score", 0.0),
            "noise_penalty": m.get("noise_penalty", 0.0),
            "final_score": m.get("score", 0.0),
            "score_per_1k_tokens": m.get("score_per_1k_tokens", 0.0),
        } for m in all_modes],
        "best_reposcry": best_reposcry["mode"],
        "best_crg": best_crg["mode"],
    }
    out_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"\nRaw data saved to {out_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
