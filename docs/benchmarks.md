# Benchmarking RepoScry

RepoScry ships simple local benchmark helpers:

- [scripts/bench.sh](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/bench.sh)
- [scripts/bench.ps1](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/bench.ps1)

The published benchmark record lives in [BENCHMARKS.md](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/BENCHMARKS.md).
Fixture definitions live in [benchmarks/fixtures.json](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/benchmarks/fixtures.json).

## Metrics

- cold index time
- warm index time
- call warmup time or `null` when call-edge persistence is folded into indexing
- architecture overview latency
- `detect_changes` latency
- `get_affected_flows` latency
- `query_graph callers_of` latency
- `semantic_search_nodes` latency
- SQLite DB size
- indexed files, symbols, imports, persisted call sites, and persisted call edges

## Output

Each benchmark run writes JSON to:

```text
benchmarks/out/latest.json
```

Use committed benchmark snapshots to detect regressions over time.
