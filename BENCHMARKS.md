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

- Captured at: `2026-05-23T13:47:00Z`
- Commit: `9c1859e`
- Fixture: `current_repo`
- Machine:
  - OS: `Microsoft Windows [version 10.0.26200.8457]`
  - CPU: `Intel64 Family 6 Model 183 Stepping 1, GenuineIntel`
  - Memory: `unknown in this restricted environment`
- Repo size:
  - files indexed: `47`
  - symbols indexed: `421`
  - imports indexed: `127`
  - persisted call sites: `3665`
  - persisted symbol call edges: `685`
  - persisted file call edges: `31`
  - total persisted file edges: `60`

### Timings

- cold index: `6634 ms`
- warm index: `6896 ms`
- call warmup: `n/a` (call-edge persistence is part of `reposcry index`)
- architecture overview: `225 ms`
- `detect_changes`: `190 ms`
- `get_affected_flows`: `164 ms`
- `query_graph callers_of`: `54 ms`
- `semantic_search_nodes`: `230 ms`
- SQLite DB size: `1810432 bytes`

### Raw artifact

The corresponding JSON snapshot is stored in [benchmarks/out/latest.json](benchmarks/out/latest.json).
