# RepoScry vs Code Review Graph — Hardened Fair Benchmark

**Task:** "add a new cache backend to reposcry-cache crate"  
**Repo:** 129 files, 15,803 lines, ~133k estimated tokens  
**Date:** 2026-05-24  

---

## Benchmark Design

Six comparison modes to eliminate "CRG was set up wrong" objections:

| Mode | What it does |
|------|-------------|
| `crg_mcp_raw` | Direct Python calls to CRG MCP tools (may fail — this is the baseline) |
| `crg_mcp_retried` | Corrected queries: absolute paths, FTS5 search, bare-name edge fallbacks |
| `crg_wiki` | All CRG wiki pages combined (general reference) |
| `crg_manual_best` | Best-effort manual: wiki + direct DB queries + detect-changes CLI |
| `reposcry_context` | RepoScry `context` command (4k/8k/16k/20k budget) |
| `reposcry_review_context` | RepoScry `get_review_context` command (4k/8k/16k/20k budget) |

## Scoring

| Score | Meaning |
|-------|---------|
| 0 | Failed, empty, wrong |
| 1 | Generic repo overview |
| 2 | Partially relevant (mentions cache) |
| 3 | Mostly correct (cache + some symbols) |
| 4 | Correct + actionable (identifies files, symbols, consumers) |
| 5 | Correct + minimal + cites exact files and symbols |

## Results

### Head-to-Head: Best per Tool

| Mode | Tokens | Score | Score/1k | Latency | Quality |
|------|--------|-------|----------|---------|---------|
| **RepoScry context (4k budget)** | **2.8k** | **5** | **1.770** | **85ms** | Perfect score, minimal tokens |
| **RepoScry review_context (4k budget)** | **3.6k** | **5** | **1.394** | **69ms** | Perfect score |
| CRG MCP retried (best CRG) | 2.1k | 3 | 1.431 | 6ms | Score 3 — partial context |
| CRG manual best | 3.1k | 3 | 0.971 | 689ms | Still only score 3 |
| CRG wiki | 12.4k | 5 | 0.405 | — | Score 5 but 4x larger than RepoScry |
| CRG MCP raw | 1.4k | 2 | 1.421 | 246ms | Failed queries |

### Quality Ranking (by Score/1k tokens)

```
  Mode                                         Score   Tokens   Sc/1k
  ──────────────────────────────────────────── ────── ──────── ───────
  reposcry_context_budget_4000                     5     2.8k   1.770   ← BEST
  crg_mcp_retried                                  3     2.1k   1.431   ← lower quality
  reposcry_review_context_budget_4000               5     3.6k   1.394
  reposcry_context_budget_8000                      5     4.7k   1.064
  reposcry_review_context_budget_8000               5     4.9k   1.029
  crg_manual_best                                   3     3.1k   0.971
  crg_wiki                                          5    12.4k   0.405
  reposcry_context_budget_16000                     5    13.8k   0.361
  reposcry_review_context_budget_20000              5    22.2k   0.225
```

### RepoScry Budget Sweep

| Budget | Mode | Tokens | Score | Score/1k | Latency |
|--------|------|--------|-------|----------|---------|
| **4k** | context | **2.8k** | **5** | **1.770** | **85ms** |
| **4k** | review_context | **3.6k** | **5** | **1.394** | **69ms** |
| 8k | context | 4.7k | 5 | 1.064 | 68ms |
| 8k | review_context | 4.9k | 5 | 1.029 | 68ms |
| 16k | context | 13.8k | 5 | 0.361 | 67ms |
| 16k | review_context | 14.3k | 5 | 0.350 | 73ms |
| 20k | context | 19.6k | 5 | 0.255 | 76ms |
| 20k | review_context | 22.2k | 5 | 0.225 | 75ms |

**RepoScry maintains Score 5 at all budgets from 4k to 20k.** Even at the lowest budget (4k), it produces only 2.8k actual tokens — beating the aggressive target of 4k–8k.

---

## Key Findings

### 1. Even with best-effort queries, CRG can't match RepoScry's task context

The `crg_mcp_retried` mode used:
- Absolute file paths (correct)
- FTS5 search
- Bare-name edge fallbacks
- Direct SQLite queries
- Correct qualified names

Despite all that, it scored **3** (mostly correct but noisy) vs RepoScry's **5** at similar token counts. CRG's architecture is optimized for **per-query MCP interactions**, not for generating a **coherent task-specific context pack**.

### 2. CRG's main weakness is context assembly

CRG's graph depth (5,803 edges vs RepoScry's 1,113) and FTS5 are strong, but:
- **No task-context command** — you must manually query 5-10 tools
- **Semantic search returns 0** without precomputed embeddings
- **Import resolution fails** on relative/bare paths
- **Symbol disambiguation fails** — `query_graph CacheDb` returns "multiple matches"

### 3. RepoScry achieves target budget

| Target | Budget | Actual tokens | Score |
|--------|--------|---------------|-------|
| Current (19.5k–21.6k) | 20k | 19.6k | 5 |
| Near-term (8k–12k) | 8k | 4.7k | 5 |
| Aggressive (4k–8k) | 4k | **2.8k** | **5** |

All targets are exceeded. RepoScry at 4k budget produces **score-5 context in 2.8k tokens (85ms)**.

### 4. Score/1k tokens is the best headline metric

| Mode | Score/1k | Tokens to score 5 |
|------|----------|-------------------|
| **RepoScry (4k budget)** | **1.770** | **2.8k** |
| CRG retried | 1.431 | N/A (max score 3) |
| CRG manual best | 0.971 | N/A (max score 3) |
| CRG wiki | 0.405 | 12.4k |

RepoScry delivers the highest quality-per-token ratio while CRG cannot reach score 5 at any token budget for task-specific context.

---

## Conclusion

**RepoScry wins on both token efficiency and code understanding quality** for the one-shot task-context use case. Even when CRG is given best-effort queries with correct paths and FTS5, it cannot produce a coherent task-specific context pack — it's designed for iterative MCP interactions, not single-command context generation.

RepoScry at 4k budget achieves **score 5 with 2.8k tokens in 85ms** — a 98% reduction from raw source while maintaining perfect task relevance.
