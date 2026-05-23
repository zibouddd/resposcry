# Benchmarking RepoScry

RepoScry ships simple local benchmark helpers:

- [scripts/bench.sh](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/bench.sh)
- [scripts/bench.ps1](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/bench.ps1)

The published benchmark record lives in [BENCHMARKS.md](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/BENCHMARKS.md).

## Metrics

- cold index time
- warm index time
- architecture overview latency
- `query_graph callers_of` latency
- `semantic_search_nodes` latency
- SQLite DB size
- indexed files, symbols, imports, and persisted call edges

## Output

Each benchmark run writes JSON to:

```text
benchmarks/out/latest.json
```

Use committed benchmark snapshots to detect regressions over time.
