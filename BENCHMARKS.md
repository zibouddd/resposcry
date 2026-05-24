# Benchmarks

This file tracks reproducible RepoScry benchmark runs.

## Method

- Run `scripts/bench.sh` on Unix-like systems or `scripts/bench.ps1` on Windows.
- Run `scripts/bench-all-fixtures.sh` or `scripts/bench-all-fixtures.ps1` to sweep every committed local fixture.
- Run `scripts/setup-benchmark-fixtures.sh` or `scripts/setup-benchmark-fixtures.ps1` once when you need local nested git history for the fixture repos.
- Capture:
  - cold lexical index time (`reposcry index --no-semantic`)
  - warm lexical index time (`reposcry index --no-semantic`)
  - semantic index refresh time with cached local vectors (`reposcry refresh-search --semantic-backend local-hash-v1`)
  - semantic index full rebuild time (`reposcry refresh-search --semantic-backend local-hash-v1 --reembed-all`)
  - call warmup time
  - architecture overview latency
  - `detect_changes` latency
  - `get_affected_flows` latency
  - `query_graph callers_of` latency
  - `semantic_search_nodes` latency
  - database size
  - indexed file/symbol/import/call counts
- Optionally capture an additional semantic backend by setting `REPOSCRY_BENCH_SEMANTIC_BACKEND`, for example `candle` or `fastembed`.
  - Backend-specific snapshots add extra refresh timings for that backend while keeping the base semantic search latency measured against `local-hash-v1`.
- Store raw JSON output under `benchmarks/out/`.
- Use `BENCH_OUT_NAME` to choose a filename explicitly, or set `REPOSCRY_BENCH_SEMANTIC_BACKEND` to emit `benchmarks/out/latest-<backend>.json`.
- Fixture inventory lives in [benchmarks/fixtures.json](benchmarks/fixtures.json).
- Fixture source trees live under [benchmarks/fixtures](benchmarks/fixtures).

## Compare against code-review-graph

Run both tools on the same working tree with:

```bash
python scripts/bench-code-review-graph.py --repo .
```

To force a real `code-review-graph build` run and fail if it is missing:

```bash
pipx install code-review-graph
python scripts/bench-code-review-graph.py --repo . --require-crg
```

The comparison runner records:

- `reposcry_cold_index_no_semantic`
- `reposcry_warm_index_no_semantic`
- `reposcry_incremental_readme_refresh_search`
- `code_review_graph_build`

Output is written to:

```text
benchmarks/out/latest-code-review-graph-compare.json
```

Use this file when making claims such as “RepoScry is faster than code-review-graph”; do not use theoretical language/runtime arguments alone.

## Fixtures

- `current_repo`: this repository
- `small_rust_repo`: committed compact Rust fixture under `benchmarks/fixtures/small_rust_repo`
- `medium_nextjs_repo`: committed Next.js-style fixture under `benchmarks/fixtures/medium_nextjs_repo`
- `mixed_monorepo`: committed mixed Rust/TypeScript monorepo fixture under `benchmarks/fixtures/mixed_monorepo`
- `large_repo`: committed larger synthetic mixed repo under `benchmarks/fixtures/large_repo`

## Latest run

- Captured at: `2026-05-23T16:04:50Z`
- Commit: `9c1859e`
- Fixture: `current_repo`
- Machine:
  - OS: `Microsoft Windows [version 10.0.26200.8457]`
  - CPU: `Intel64 Family 6 Model 183 Stepping 1, GenuineIntel`
  - Memory: `unknown in this restricted environment`
- Repo size:
  - files indexed: `103`
  - symbols indexed: `618`
  - imports indexed: `175`
  - persisted call sites: `5757`
  - persisted symbol call edges: `1084`
  - persisted file call edges: `56`
  - total persisted file edges: `95`

### Timings

- cold lexical index (`reposcry index --no-semantic`): `10655 ms`
- warm lexical index (`reposcry index --no-semantic`): `7599 ms`
- semantic index refresh (`reposcry refresh-search --semantic-backend local-hash-v1`): `5669 ms`
- semantic index rebuild (`reposcry refresh-search --semantic-backend local-hash-v1 --reembed-all`): `5148 ms`
- call warmup: `4241 ms`
- architecture overview: `111 ms`
- `detect_changes`: `188 ms`
- `get_affected_flows`: `177 ms`
- `query_graph callers_of`: `79 ms`
- `semantic_search_nodes`: `498 ms`
- SQLite DB size: `3305472 bytes`

### Raw artifact

The corresponding JSON snapshot is stored in [benchmarks/out/latest.json](benchmarks/out/latest.json).

## Fixture sweep

All committed local fixture classes now have measured snapshots from `2026-05-23` on the same machine.

| Fixture | Artifact | Files | Symbols | Cold index | Warm index | Search |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `small_rust_repo` | [benchmarks/out/latest-small_rust_repo.json](benchmarks/out/latest-small_rust_repo.json) | `6` | `9` | `1958 ms` | `1620 ms` | `71 ms` |
| `medium_nextjs_repo` | [benchmarks/out/latest-medium_nextjs_repo.json](benchmarks/out/latest-medium_nextjs_repo.json) | `11` | `9` | `235 ms` | `226 ms` | `69 ms` |
| `mixed_monorepo` | [benchmarks/out/latest-mixed_monorepo.json](benchmarks/out/latest-mixed_monorepo.json) | `10` | `6` | `190 ms` | `237 ms` | `70 ms` |
| `large_repo` | [benchmarks/out/latest-large_repo.json](benchmarks/out/latest-large_repo.json) | `21` | `14` | `458 ms` | `322 ms` | `66 ms` |

### Fixture notes

- The committed fixtures are intentionally lightweight so they can run in a restricted local environment without downloading app dependencies.
- The fixture setup scripts create nested `.git` directories so diff-based commands such as `detect_changes main HEAD` and `get_affected_flows main HEAD` have stable git refs.
- The latest fixture sweep also exercises import-resolved TypeScript workspace calls, so `medium_nextjs_repo` and `mixed_monorepo` now persist symbol call edges instead of falling back to zero-edge placeholders.

## Backend snapshots

Backend-specific semantic refresh runs can be captured into separate artifacts instead of overwriting the default snapshot:

```powershell
$env:REPOSCRY_BENCH_SEMANTIC_BACKEND='fastembed'
./scripts/bench.ps1

$env:REPOSCRY_BENCH_SEMANTIC_BACKEND='candle'
./scripts/bench.ps1
```

Those runs write:

- [benchmarks/out/latest-fastembed.json](benchmarks/out/latest-fastembed.json)

### Verified backend snapshot

- `fastembed` snapshot captured at `2026-05-23T15:06:15Z`
  - extra backend refresh: `28390 ms`
  - extra backend rebuild: `16361 ms`
  - artifact: [benchmarks/out/latest-fastembed.json](benchmarks/out/latest-fastembed.json)

### Candle timeout evidence

- An unrestricted `candle` benchmark run was attempted on `2026-05-23` with:
  - `REPOSCRY_BENCH_SEMANTIC_BACKEND=candle`
  - `REPOSCRY_CANDLE_MODEL=qwen3`
- The run did not finish within the `904040 ms` timeout on this machine, so no trustworthy candle JSON snapshot is committed.
- This is still useful evidence: the default `candle/qwen3` refresh path is materially more expensive operationally than the default local-hash or the captured `fastembed` path in this environment.
