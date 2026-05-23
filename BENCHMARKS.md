# Benchmarks

This file tracks reproducible RepoScry benchmark runs.

## Method

- Run `scripts/bench.sh` on Unix-like systems or `scripts/bench.ps1` on Windows.
- Capture:
  - cold index time
  - warm index time
  - architecture overview latency
  - `query_graph callers_of` latency
  - `semantic_search_nodes` latency
  - database size
  - indexed file/symbol/import/call counts
- Store raw JSON output under `benchmarks/out/`.

## Fixtures

- `current_repo`: this repository
- `small_rust_repo`: TODO
- `medium_nextjs_repo`: TODO
- `mixed_monorepo`: TODO
- `large_repo`: TODO

## Latest run

- Captured at: `2026-05-23T13:21:41Z`
- Commit: `9c1859e`
- Fixture: `current_repo`
- Machine:
  - OS: `Microsoft Windows [version 10.0.26200.8457]`
  - CPU: `Intel64 Family 6 Model 183 Stepping 1, GenuineIntel`
  - Memory: `unknown in this restricted environment`
- Repo size:
  - files indexed: `46`
  - symbols indexed: `392`
  - imports indexed: `118`
  - persisted call sites: `3367`
  - persisted symbol call edges: `635`
  - persisted file call edges: `31`

### Timings

- cold index: `4718 ms`
- warm index: `4500 ms`
- architecture overview: `41 ms`
- `query_graph callers_of`: `31 ms`
- `semantic_search_nodes`: `27 ms`
- SQLite DB size: `1634304 bytes`

### Raw artifact

The corresponding JSON snapshot is stored in [benchmarks/out/latest.json](benchmarks/out/latest.json).
