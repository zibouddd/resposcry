# RepoScry

[![CI](https://github.com/zibouddd/resposcry/actions/workflows/ci.yml/badge.svg)](https://github.com/zibouddd/resposcry/actions/workflows/ci.yml)
[![Release](https://github.com/zibouddd/resposcry/actions/workflows/release.yml/badge.svg)](https://github.com/zibouddd/resposcry/actions/workflows/release.yml)
[![Crates.io](https://img.shields.io/crates/v/reposcry-cli.svg)](https://crates.io/crates/reposcry-cli)
[![Crates.io downloads](https://img.shields.io/crates/d/reposcry-cli.svg)](https://crates.io/crates/reposcry-cli)
[![GitHub release downloads](https://img.shields.io/github/downloads/zibouddd/resposcry/total.svg)](https://github.com/zibouddd/resposcry/releases)
[![GitHub stars](https://img.shields.io/github/stars/zibouddd/resposcry?style=social)](https://github.com/zibouddd/resposcry/stargazers)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

RepoScry is a local code review graph engine for repository indexing, impact analysis, AI context generation, CRG-compatible queries, graph export, watch-mode updates, and MCP tool serving.

The default workflow is optimized for AI coding agents: keep a fast lexical/code graph hot, update changed files incrementally, and run semantic/vector refresh only when needed.

## Why RepoScry

- Rust-native local binaries for macOS, Linux, and Windows.
- Incremental update loop for agents and editors.
- Separate semantic refresh path so large embedding models do not block normal indexing.
- CRG-compatible commands for code-review-graph style workflows.
- MCP and MCP-plus stdio servers for AI coding tools.
- Graph export to JSON, GraphML, and lightweight HTML.

## Project stats

| Metric | Badge |
| --- | --- |
| Crates.io version | [![Crates.io](https://img.shields.io/crates/v/reposcry-cli.svg)](https://crates.io/crates/reposcry-cli) |
| Crates.io downloads | [![Crates.io downloads](https://img.shields.io/crates/d/reposcry-cli.svg)](https://crates.io/crates/reposcry-cli) |
| GitHub release downloads | [![GitHub release downloads](https://img.shields.io/github/downloads/zibouddd/resposcry/total.svg)](https://github.com/zibouddd/resposcry/releases) |
| GitHub stars | [![GitHub stars](https://img.shields.io/github/stars/zibouddd/resposcry?style=social)](https://github.com/zibouddd/resposcry/stargazers) |

## Binaries

RepoScry ships multiple CLI binaries:

| Binary | Purpose |
| --- | --- |
| `reposcry` | Main CLI for indexing, graph analysis, context packs, reports, validation, search, MCP, and CRG-compatible commands. |
| `reposcry-update` | Fast incremental updater for changed files or explicit file paths. Intended for edit loops and hooks. |
| `reposcry-watch` | Polling watch mode that runs incremental updates when Git reports changed files. |
| `reposcry-export` | Exports the cached graph as JSON, GraphML, or a lightweight HTML report. |
| `reposcry-mcp-plus` | Expanded read-only MCP server for graph inspection tools. |

## Install

### macOS / Linux

Download the installer, inspect it if needed, then run it:

```bash
curl -fsSLO https://raw.githubusercontent.com/zibouddd/resposcry/main/install.sh
bash install.sh
```

The installer downloads the release archive, verifies its SHA-256 checksum, and installs all RepoScry binaries.

Pin a tagged release:

```bash
REPOSCRY_VERSION=v0.1.0 bash install.sh
```

### Windows PowerShell

Download the installer, inspect it if needed, then run it:

```powershell
iwr https://raw.githubusercontent.com/zibouddd/resposcry/main/install.ps1 -OutFile install.ps1
.\install.ps1
```

Pin a tagged release:

```powershell
$env:REPOSCRY_VERSION='v0.1.0'
.\install.ps1
```

### From source

```bash
cargo install --path crates/reposcry-cli --force
```

After crates.io publication:

```bash
cargo install reposcry-cli
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

Run a polling watch loop during an editor or agent session:

```bash
reposcry-watch --repo . --base main --refresh-search --skip-warm-calls
```

Run one watch iteration for hooks or CI:

```bash
reposcry-watch --repo . --once --json
```

Useful incremental flags:

| Flag | Effect |
| --- | --- |
| `--changed` | Include files from `git status --porcelain` and `git diff --name-only <base>`. |
| `--file <path>` | Update an explicit file. Can be repeated. |
| `--base <ref>` | Diff base for `--changed`. Defaults to `HEAD`. Use `main` for branch work. |
| `--skip-warm-calls` | Skip call-edge warmup for the fastest possible update. |
| `--refresh-search` | Rebuild lexical search documents after the file update. Semantic vectors are not rebuilt. |

Helper scripts:

```bash
./scripts/reposcry-watch.sh main
```

```powershell
./scripts/reposcry-watch.ps1 main
```

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

## Graph export

Export the cached graph after indexing:

```bash
reposcry-export --repo . --format json --output .reposcry/graph.json
reposcry-export --repo . --format graphml --output .reposcry/graph.graphml
reposcry-export --repo . --format html --output .reposcry/graph.html
```

Include symbol nodes and symbol-level calls:

```bash
reposcry-export --repo . --format json --symbols --output .reposcry/graph-symbols.json
```

## Language support

RepoScry recognizes a broad language matrix. The first group has parser-backed extraction today; the second group is indexed at file/path/LOC/language level until deeper parser extraction is added.

| Support level | Languages |
| --- | --- |
| Parser-backed | Rust, TypeScript, TSX, JavaScript, JSX, Python, JSON, TOML, YAML |
| File-level indexed | Go, Java, C#, C, C++, Kotlin, Swift, PHP, Ruby, Lua, Dart, Scala, Vue, Svelte, Nix, PowerShell, Markdown, CSS, HTML, SQL |

Planned parser priorities:

1. Go
2. Java
3. C#
4. Vue / Svelte
5. Kotlin / Swift

See [docs/language-support.md](docs/language-support.md).

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

Run the CRG-compatible MCP stdio server:

```bash
reposcry --repo /path/to/repo mcp
```

Run the expanded read-only MCP-plus server:

```bash
reposcry-mcp-plus --repo /path/to/repo
```

Example client configuration:

```json
{
  "mcpServers": {
    "reposcry": {
      "command": "reposcry",
      "args": ["--repo", "/path/to/repo", "mcp"]
    },
    "reposcry-plus": {
      "command": "reposcry-mcp-plus",
      "args": ["--repo", "/path/to/repo"]
    }
  }
}
```

Supported MCP methods:

- `initialize`
- `tools/list`
- `tools/call`

`reposcry-mcp-plus` tools:

- `get_graph_summary`
- `list_languages`
- `list_files`
- `list_symbols`
- `get_file_neighborhood`
- `export_graph_json`

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

## Downloads

RepoScry is distributed through two channels:

| Channel | Purpose |
| --- | --- |
| GitHub Releases | Standalone binaries for macOS, Linux, and Windows. |
| crates.io | Source-based installation through Cargo after crate publication. |

Download badges:

[![GitHub release downloads](https://img.shields.io/github/downloads/zibouddd/resposcry/total.svg)](https://github.com/zibouddd/resposcry/releases)
[![Crates.io downloads](https://img.shields.io/crates/d/reposcry-cli.svg)](https://crates.io/crates/reposcry-cli)

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

## Star history

[![Star History Chart](https://api.star-history.com/svg?repos=zibouddd/resposcry&type=Date)](https://www.star-history.com/#zibouddd/resposcry&Date)

## Release

Create a tagged release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The release workflow packages all binaries and publishes checksums.

## crates.io publishing

Before publishing, make sure every workspace crate has package metadata and versioned internal dependencies.

Recommended dry run order:

```bash
cargo publish -p reposcry-graph --dry-run
cargo publish -p reposcry-cache --dry-run
cargo publish -p reposcry-git --dry-run
cargo publish -p reposcry-indexer --dry-run
cargo publish -p reposcry-rules --dry-run
cargo publish -p reposcry-context --dry-run
cargo publish -p reposcry-report --dry-run
cargo publish -p reposcry-cli --dry-run
```

Publish order:

```bash
cargo publish -p reposcry-graph
cargo publish -p reposcry-cache
cargo publish -p reposcry-git
cargo publish -p reposcry-indexer
cargo publish -p reposcry-rules
cargo publish -p reposcry-context
cargo publish -p reposcry-report
cargo publish -p reposcry-cli
```

GitHub Actions publishing should use a `CRATES_IO_TOKEN` repository secret.

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
- [docs/language-support.md](docs/language-support.md)

## Limitations

- Dynamic imports, reflection, and framework runtime behavior are under-approximated.
- Call resolution still uses heuristics when multiple symbol matches are plausible.
- Newly recognized languages without parser support are indexed at file/path/LOC/language level only.
- Diff-based commands such as `detect_changes main HEAD` inspect git refs, not unstaged working tree edits.
- Heavy semantic backends such as Candle/Qwen3 can be slow on first run because model download and vector generation are outside the fast edit loop.
- End-to-end release publication requires a tagged GitHub release workflow run.
