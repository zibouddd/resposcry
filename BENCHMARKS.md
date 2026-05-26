# Benchmarks

Captured: `2026-05-26T09:33:39Z`

## Machine

| Property | Value |
|---|---|
| OS | Microsoft Windows [version 10.0.26200.8457] |
| CPU | Intel64 Family 6 Model 183 Stepping 1, GenuineIntel |
| Profile | release |

## Fixture: current_repo

| Property | Value |
|---|---|
| Path | reposcry |
| Size | small_rust_workspace |
| Notes | Primary smoke and regression fixture committed with the repository |

## Metrics

| Metric | Value |
|---|---|
| cold_index_ms | 1916 |
| warm_index_ms | 1086 |
| warm_index_speedup | 1.76x |
| semantic_index_reuse_ms | 377 |
| semantic_index_reembed_ms | 414 |
| call_warmup_ms | 181 |
| architecture_overview_ms | 95 |
| detect_changes_ms | 175 |
| affected_flows_ms | 174 |
| query_graph_callers_ms | 77 |
| context_pack_ms | 75 |
| semantic_search_ms | 102 |
| db_size_bytes | 3,162,112 (~3 MB) |

## Indexed Content

| Metric | Value |
|---|---|
| files_indexed | 118 |
| symbols_indexed | 631 |
| imports_indexed | 221 |
| persisted_call_sites | 6,301 |
| persisted_symbol_call_edges | 1,150 |
| persisted_file_call_edges | 80 |
| total_edges | 123 |

## Interpretation

- **Cold index** (1916 ms): full lexical parse from scratch.
- **Warm index** (1086 ms): incremental re-parse — **1.76x faster** than cold.
- **Semantic refresh** (377 ms): incremental search vector update.
- **Semantic re-embed** (414 ms): full re-embedding of all symbols.
- **Context pack** (75 ms): budgeted context assembly (20k budget).
- **Architecture overview** (95 ms): structure extraction.
- **Detect changes** (175 ms): diff analysis against `main`.
- **Query graph callers** (77 ms): caller-of lookup.

## Running

```powershell
$env:REPOSCRY_BENCH_RELEASE = "1"
scripts/bench.ps1
```

For all fixtures:

```powershell
scripts/bench-all-fixtures.ps1
```
