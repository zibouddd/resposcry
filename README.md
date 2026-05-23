# RepoScry

[![CI](https://github.com/zibouddd/resposcry/actions/workflows/ci.yml/badge.svg)](https://github.com/zibouddd/resposcry/actions/workflows/ci.yml)
[![Release](https://github.com/zibouddd/resposcry/actions/workflows/release.yml/badge.svg)](https://github.com/zibouddd/resposcry/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

RepoScry is a local code review graph engine for repository indexing, impact analysis, AI context generation, CRG-compatible queries, and MCP tool serving.

## Package naming

- Canonical CLI: `reposcry`

`reposcry` is the user-facing command for indexing, graph analysis, CRG-compatible queries, and MCP serving.

## Install

From source:

```bash
cargo install --path crates/reposcry-cli --force
```

From release artifact:

```bash
curl -fsSL https://raw.githubusercontent.com/zibouddd/resposcry/main/install.sh | bash
```

The installer downloads the release archive and verifies its SHA-256 checksum before installing `reposcry`.

On Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/zibouddd/resposcry/main/install.ps1 | iex
```

Pin a specific tagged release:

```bash
curl -fsSL https://raw.githubusercontent.com/zibouddd/resposcry/main/install.sh | REPOSCRY_VERSION=v0.1.0 bash
```

Pin a specific tagged release on Windows:

```powershell
$env:REPOSCRY_VERSION='v0.1.0'
irm https://raw.githubusercontent.com/zibouddd/resposcry/main/install.ps1 | iex
```

Update an existing source install:

```bash
cargo install --path crates/reposcry-cli --force
```

## Quick start

Initialize and index the current repository:

```bash
reposcry init
reposcry index
reposcry warm-calls
reposcry refresh-search --semantic-backend local-hash-v1
reposcry stats
```

Skip semantic vector refresh when you only want a fast lexical/code graph refresh:

```bash
reposcry index --no-semantic
```

Build a focused context pack for an editing task:

```bash
reposcry context "fix dependency graph rebuild" --strict --budget 20000 --format markdown > .reposcry/AI_CONTEXT.md
```

Inspect graph impact before editing:

```bash
reposcry explain crates/reposcry-cli/src/main.rs
reposcry deps crates/reposcry-cli/src/main.rs
reposcry rdeps crates/reposcry-cache/src/db.rs
```

Validate after editing:

```bash
reposcry validate main
```

## Full index workflow

Use the full-index command when you want one command for an install-time index pass:

```bash
reposcry --repo . index-full
```

Choose a semantic backend explicitly or force a full vector rebuild:

```bash
reposcry refresh-search --semantic-backend fastembed
reposcry refresh-search --semantic-backend candle --reembed-all
```

If you already indexed files and only want to rebuild persisted call edges:

```bash
reposcry --repo . warm-calls
```

If you only want to rebuild search documents and vectors from the cached graph:

```bash
reposcry --repo . refresh-search --semantic-backend local-hash-v1
```

## CRG-compatible commands

RepoScry ships a CRG-compatible command surface directly from `reposcry`:

```bash
reposcry --repo . get_architecture_overview --format json
reposcry --repo . query_graph "callers_of rebuild_graph"
reposcry --repo . query_graph "tests_for parse_rust"
reposcry --repo . get_impact_radius rebuild_graph --depth 4
reposcry --repo . get_affected_flows main HEAD
reposcry --repo . semantic_search_nodes "cache database calls" --limit 20
reposcry --repo . refactor_tool rename parse_rust parse_rust_v2
```

## MCP setup

Run the MCP-compatible stdio server:

```bash
reposcry mcp --repo /path/to/repo
```

Example client configuration:

```json
{
  "mcpServers": {
    "reposcry": {
      "command": "reposcry",
      "args": ["mcp", "--repo", "/path/to/repo"]
    }
  }
}
```

Supported MCP methods:

- `initialize`
- `tools/list`
- `tools/call`

## What gets indexed

- files
- symbols
- imports
- file-level import edges
- call sites
- symbol-level call edges
- local full-text search documents

The SQLite cache lives in:

```text
.reposcry/reposcry.db
```

## Semantic backends

Semantic search works without external services by default with `local-hash-v1`.

Additional configured backends:

- `ollama`
- `fastembed`
- `candle`

Index-time vector refresh now reuses cached vectors for unchanged node ids on a per-backend basis. Use `--reembed-all` when you want to discard and rebuild vectors for the selected backend, or `--no-semantic` to rebuild only lexical search documents.
Use `reposcry refresh-search` when you want to rebuild search documents and vectors from the cached graph without rescanning the repository.

`fastembed` now defaults its writable cache under `.reposcry/hf-home` when `HF_HOME` is not set. You can override that location with `REPOSCRY_FASTEMBED_CACHE_DIR`.
`candle` uses the same writable Hugging Face cache root and supports:
- `REPOSCRY_CANDLE_MODEL=qwen3` with default repo `Qwen/Qwen3-Embedding-0.6B`
- `REPOSCRY_CANDLE_MODEL=nomic-v2-moe` with repo `nomic-ai/nomic-embed-text-v2-moe`

You can override the repo with `REPOSCRY_CANDLE_REPO` and the cache location with `REPOSCRY_CANDLE_CACHE_DIR`.

Examples:

```bash
set REPOSCRY_SEMANTIC_BACKEND=fastembed
set REPOSCRY_FASTEMBED_MODEL=AllMiniLML6V2
reposcry index
reposcry semantic_search_nodes "cache database calls" --semantic
```

```bash
set REPOSCRY_SEMANTIC_BACKEND=ollama
set REPOSCRY_OLLAMA_MODEL=nomic-embed-text
reposcry semantic_search_nodes "cache database calls" --semantic
```

```bash
set REPOSCRY_SEMANTIC_BACKEND=candle
set REPOSCRY_CANDLE_MODEL=qwen3
reposcry semantic_search_nodes "cache database calls" --semantic
```

## Benchmarks

Run local benchmarks with:

```bash
bash scripts/bench.sh
```

or on Windows:

```powershell
./scripts/bench.ps1
```

Run the full local fixture sweep with:

```powershell
./scripts/bench-all-fixtures.ps1
```

Published benchmark notes live in [BENCHMARKS.md](BENCHMARKS.md).

## Release smoke

Run the local release/install smoke path with:

```powershell
./scripts/smoke-release.ps1
```

or on Unix-like systems:

```bash
bash scripts/smoke-release.sh
```

## Documentation

- [docs/architecture.md](docs/architecture.md)
- [docs/mcp.md](docs/mcp.md)
- [docs/benchmarks.md](docs/benchmarks.md)
- [docs/code-review-graph-compat.md](docs/code-review-graph-compat.md)

## Limitations

- Dynamic imports, reflection, and framework runtime behavior are under-approximated.
- Call resolution still uses heuristics when multiple symbol matches are plausible.
- Diff-based commands such as `detect_changes main HEAD` inspect git refs, not unstaged working tree edits.
- Release-install verification currently covers checksum validation and local packaging logic; end-to-end GitHub release publication still requires exercising the workflow on GitHub.
