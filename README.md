# RepoScry

[![CI](https://github.com/zibouddd/resposcry/actions/workflows/ci.yml/badge.svg)](https://github.com/zibouddd/resposcry/actions/workflows/ci.yml)
[![Release](https://github.com/zibouddd/resposcry/actions/workflows/release.yml/badge.svg)](https://github.com/zibouddd/resposcry/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

RepoScry is a local code review graph engine for repository indexing, impact analysis, AI context generation, CRG-compatible queries, and MCP tool serving.

The default workflow is optimized for AI coding agents: keep a fast lexical/code graph hot, update changed files incrementally, and run semantic/vector refresh only when needed.

## Binaries

RepoScry ships two CLI binaries:

| Binary | Purpose |
| --- | --- |
| `reposcry` | Full repository graph, context, report, validation, search, MCP, and CRG-compatible command surface. |
| `reposcry-update` | Fast incremental updater for changed files or explicit file paths. Intended for edit loops and hooks. |

## Install

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/zibouddd/resposcry/main/install.sh | bash
```

The installer downloads the release archive, verifies its SHA-256 checksum, and installs both `reposcry` and `reposcry-update`.

Pin a tagged release:

```bash
curl -fsSL https://raw.githubusercontent.com/zibouddd/resposcry/main/install.sh | REPOSCRY_VERSION=v0.1.0 bash
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/zibouddd/resposcry/main/install.ps1 | iex
```

Pin a tagged release:

```powershell
$env:REPOSCRY_VERSION='v0.1.0'
irm https://raw.githubusercontent.com/zibouddd/resposcry/main/install.ps1 | iex
```

### From source

```bash
cargo install --path crates/reposcry-cli --force
```

## Fast edit loop

Use this loop during normal coding. It avoids semantic/vector work by default.

```bash
reposcry init
reposcry index --no-semantic
reposcry context "fix dependency graph rebuild" --strict --budget 20000 --format markdown > .reposcry/AI_CONTEXT.md
```

After editing, update only changed files:

```bash
reposcry-update --changed --base main
reposcry validate main HEAD
```

Update explicit files instead of asking Git for a diff:

```bash
reposcry-update --file crates/reposcry-cli/src/main.rs --refresh-search
```

Useful incremental flags:

| Flag | Effect |
| --- | --- |
| `--changed` | Include files from `git status --porcelain` and `git diff --name-only <base>`. |
| `--file <path>` | Update an explicit file. Can be repeated. |
| `--base <ref>` | Diff base for `--changed`. Defaults to `HEAD`. Use `main` for branch work. |
| `--skip-warm-calls` | Skip call-edge warmup for the fastest possible update. |
| `--refresh-search` | Rebuild lexical search documents after the file update. Semantic vectors are not rebuilt. |

## Full index workflow

Use a full index when setting up a repository or after large structural changes.

```bash
reposcry --repo . index --no-semantic
reposcry --repo . warm-calls
reposcry --repo . stats
```

`index-full` emits a JSON summary for automation:

```bash
reposcry --repo . index-full --no-semantic
```

## Semantic refresh is separate

Semantic search is intentionally outside the normal edit loop.

```bash
reposcry refresh-search --semantic-backend local-hash-v1
reposcry semantic_search_nodes "cache database calls" --semantic --semantic-backend local-hash-v1
```

Heavier backends are opt-in:

```bash
REPOSCRY_SEMANTIC_BACKEND=fastembed reposcry refresh-search --semantic-backend fastembed
REPOSCRY_SEMANTIC_BACKEND=candle REPOSCRY_CANDLE_MODEL=qwen3 reposcry refresh-search --semantic-backend candle
```

Use `--reembed-all` when you want to discard cached vectors for the selected backend:

```bash
reposcry refresh-search --semantic-backend fastembed --reembed-all
```

## CRG-compatible commands

RepoScry exposes a code-review-graph-compatible command surface:

```bash
reposcry --repo . get_architecture_overview --format json
reposcry --repo . query_graph "callers_of rebuild_graph"
reposcry --repo . query_graph "tests_for parse_rust"
reposcry --repo . get_impact_radius rebuild_graph --depth 4
reposcry --repo . get_affected_flows main HEAD
reposcry --repo . semantic_search_nodes "cache database calls" --limit 20
reposcry --repo . refactor_tool rename parse_rust parse_rust_v2
```

## Agent setup: Codex, Claude, Cursor, Copilot, and more

Install project instructions and helper scripts for one platform:

```bash
reposcry install --platform codex
reposcry install --platform claude
reposcry install --platform cursor
```

Install all supported instruction templates:

```bash
reposcry install --platform all
```

Generated integrations instruct agents to:

1. run `reposcry index --no-semantic` before broad exploration;
2. create `.reposcry/AI_CONTEXT.md` for the current task;
3. inspect dependencies and reverse dependencies before edits;
4. run `reposcry-update --changed --base main` after edit batches;
5. validate with `reposcry validate main HEAD`.

## MCP setup

Run the MCP-compatible stdio server:

```bash
reposcry --repo /path/to/repo mcp
```

Example client configuration:

```json
{
  "mcpServers": {
    "reposcry": {
      "command": "reposcry",
      "args": ["--repo", "/path/to/repo", "mcp"]
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
- optional semantic vectors

The SQLite cache lives in:

```text
.reposcry/reposcry.db
```

## Semantic backends

Default backend:

- `local-hash-v1`

Additional configured backends:

- `ollama`
- `fastembed`
- `candle`

Environment variables:

| Backend | Variables |
| --- | --- |
| `ollama` | `REPOSCRY_OLLAMA_URL`, `REPOSCRY_OLLAMA_MODEL` |
| `fastembed` | `REPOSCRY_FASTEMBED_MODEL`, `REPOSCRY_FASTEMBED_CACHE_DIR` |
| `candle` | `REPOSCRY_CANDLE_MODEL`, `REPOSCRY_CANDLE_REPO`, `REPOSCRY_CANDLE_CACHE_DIR`, `REPOSCRY_CANDLE_MAX_LENGTH` |

`fastembed` and `candle` use `.reposcry/hf-home` as a writable Hugging Face cache root when `HF_HOME` is not set.

## Benchmarks

Run RepoScry local benchmarks:

```bash
bash scripts/bench.sh
```

On Windows:

```powershell
./scripts/bench.ps1
```

Compare against `code-review-graph` on the same repository:

```bash
python scripts/bench-code-review-graph.py --repo .
```

To require a real `code-review-graph build` run:

```bash
pipx install code-review-graph
python scripts/bench-code-review-graph.py --repo . --require-crg
```

The comparison runner writes JSON to:

```text
benchmarks/out/latest-code-review-graph-compare.json
```

Published notes live in [BENCHMARKS.md](BENCHMARKS.md).

## Release smoke

Run the local release/install smoke path with:

```bash
bash scripts/smoke-release.sh
```

On Windows:

```powershell
./scripts/smoke-release.ps1
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
- Heavy semantic backends such as Candle/Qwen3 can be slow on first run because model download and vector generation are outside the fast edit loop.
- End-to-end release publication requires a tagged GitHub release workflow run.
