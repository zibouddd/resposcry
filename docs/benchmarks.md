# Benchmarking RepoScry

RepoScry ships simple local benchmark helpers:

- [scripts/bench.sh](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/bench.sh)
- [scripts/bench.ps1](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/bench.ps1)
- [scripts/bench-all-fixtures.sh](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/bench-all-fixtures.sh)
- [scripts/bench-all-fixtures.ps1](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/bench-all-fixtures.ps1)
- [scripts/setup-benchmark-fixtures.sh](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/setup-benchmark-fixtures.sh)
- [scripts/setup-benchmark-fixtures.ps1](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/scripts/setup-benchmark-fixtures.ps1)

The published benchmark record lives in [BENCHMARKS.md](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/BENCHMARKS.md).
Fixture definitions live in [benchmarks/fixtures.json](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/benchmarks/fixtures.json).
Fixture source trees live under [benchmarks/fixtures](F:/CODING/2-PROJETS-EN-COURS/CodeReviewGraphX/benchmarks/fixtures).

## Metrics

- cold lexical index time measured with `reposcry index --no-semantic`
- warm lexical index time measured with `reposcry index --no-semantic`
- semantic index refresh time measured with `reposcry refresh-search --semantic-backend local-hash-v1`
- semantic index full rebuild time measured with `reposcry refresh-search --semantic-backend local-hash-v1 --reembed-all`
- call warmup time measured with `reposcry warm-calls`
- architecture overview latency
- `detect_changes` latency
- `get_affected_flows` latency
- `query_graph callers_of` latency
- `semantic_search_nodes` latency
- SQLite DB size
- indexed files, symbols, imports, persisted call sites, and persisted call edges

Set `REPOSCRY_BENCH_SEMANTIC_BACKEND` when you want the benchmark helpers to capture an additional backend-specific semantic index pass such as `fastembed` or `candle`.
That extra backend setting only affects the additional backend refresh timings; the base `semantic_search_nodes` latency remains pinned to `local-hash-v1` for apples-to-apples comparisons.

## Output

The default current-repo benchmark writes JSON to:

```text
benchmarks/out/latest.json
```

When `REPOSCRY_BENCH_FIXTURE` is set, the helpers emit fixture-specific artifacts named `benchmarks/out/latest-<fixture>.json`.
When `REPOSCRY_BENCH_SEMANTIC_BACKEND` is also set, the helpers emit `benchmarks/out/latest-<fixture>-<backend>.json`.
For current-repo backend runs, the helpers emit `benchmarks/out/latest-<backend>.json`.
You can override the filename directly with `BENCH_OUT_NAME`.
Backend-specific runs may still require unrestricted model/cache access, and very heavy backends such as the default `candle/qwen3` configuration can exceed practical benchmark timeouts on this machine.

Use committed benchmark snapshots to detect regressions over time.
