# RepoScry

[![CI](https://github.com/zibouddd/resposcry/actions/workflows/ci.yml/badge.svg)](https://github.com/zibouddd/resposcry/actions/workflows/ci.yml)
[![Release](https://github.com/zibouddd/resposcry/actions/workflows/release.yml/badge.svg)](https://github.com/zibouddd/resposcry/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

RepoScry is a local code review graph engine for repository indexing, impact analysis, AI context generation, CRG-compatible queries, and MCP tool serving.

## Package naming

- Canonical CLI: `reposcry`

`reposcry` is the primary user-facing command for indexing, graph analysis, CRG-compatible queries, and MCP serving. Compatibility binaries may still exist for migration, but the documented interface is `reposcry`.

## Install

From source:

```bash
cargo install --path crates/reposcry-cli --force
```

From release artifact:

```bash
curl -fsSL https://raw.githubusercontent.com/zibouddd/resposcry/main/install.sh | bash
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
reposcry stats
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

## Benchmarks

Run local benchmarks with:

```bash
./scripts/bench.sh
```

or on Windows:

```powershell
./scripts/bench.ps1
```

Published benchmark notes live in [BENCHMARKS.md](BENCHMARKS.md).

## Documentation

- [docs/architecture.md](docs/architecture.md)
- [docs/mcp.md](docs/mcp.md)
- [docs/benchmarks.md](docs/benchmarks.md)
- [docs/code-review-graph-compat.md](docs/code-review-graph-compat.md)

## Limitations

- Dynamic imports, reflection, and framework runtime behavior are under-approximated.
- Call resolution still uses heuristics when multiple symbol matches are plausible.
- Diff-based commands such as `detect_changes main HEAD` inspect git refs, not unstaged working tree edits.
- Some older examples and wrappers may still mention compatibility binaries; prefer `reposcry`.
