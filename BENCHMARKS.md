# Benchmarks

This file tracks reproducible RepoScry benchmark runs.

## Method

- Run `scripts/bench.sh` on Unix-like systems or `scripts/bench.ps1` on Windows.
- Capture:
  - cold index time
  - warm index time
  - call warmup time
  - architecture overview latency
  - `detect_changes` latency
  - `get_affected_flows` latency
  - `query_graph callers_of` latency
  - `semantic_search_nodes` latency
  - database size
  - indexed file/symbol/import/call counts
- Store raw JSON output under `benchmarks/out/`.
- Fixture inventory lives in [benchmarks/fixtures.json](benchmarks/fixtures.json).

## Fixtures

- `current_repo`: this repository
- `small_rust_repo`: external small Rust fixture, described in `benchmarks/fixtures.json`
- `medium_nextjs_repo`: external medium Next.js fixture, described in `benchmarks/fixtures.json`
- `mixed_monorepo`: external mixed-language fixture, described in `benchmarks/fixtures.json`
- `large_repo`: external large-repo fixture, described in `benchmarks/fixtures.json`

## Latest run

- Captured at: `2026-05-23T13:50:32Z`
- Commit: `9c1859e`
- Fixture: `current_repo`
- Machine:
  - OS: `Microsoft Windows [version 10.0.26200.8457]`
  - CPU: `Intel64 Family 6 Model 183 Stepping 1, GenuineIntel`
  - Memory: `unknown in this restricted environment`
- Repo size:
  - files indexed: `47`
  - symbols indexed: `423`
  - imports indexed: `127`
  - persisted call sites: `3680`
  - persisted symbol call edges: `690`
  - persisted file call edges: `31`
  - total persisted file edges: `60`

### Timings

- cold index: `10919 ms`
- warm index: `8249 ms`
- call warmup: `2517 ms`
- architecture overview: `59 ms`
- `detect_changes`: `152 ms`
- `get_affected_flows`: `143 ms`
- `query_graph callers_of`: `38 ms`
- `semantic_search_nodes`: `233 ms`
- SQLite DB size: `1908736 bytes`

### Raw artifact

The corresponding JSON snapshot is stored in [benchmarks/out/latest.json](benchmarks/out/latest.json).
