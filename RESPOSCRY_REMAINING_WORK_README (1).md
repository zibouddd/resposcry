# RepoScry Remaining Work Roadmap

This README tracks the remaining work needed to move RepoScry from a CRG-compatible implementation to a stronger, production-grade code review graph engine.

## Current baseline

Implemented so far:

- Canonical `reposcry` CLI with CRG-compatible commands.
- CRG-style commands:
  - `detect_changes`
  - `get_review_context`
  - `get_impact_radius`
  - `get_affected_flows`
  - `query_graph`
  - `semantic_search_nodes`
  - `get_architecture_overview`
  - `refactor_tool`
- Kebab-case command aliases.
- Minimal MCP-compatible stdio mode.
- Runtime heuristic `Calls` edges.
- Persistent `calls` table in SQLite.
- `reposcry warm-calls` call-site warmup command.
- `reposcry index-full` wrapper command.
- Basic CI workflow.
- Basic benchmark script.

Remaining work is mostly about accuracy, persistence quality, search quality, flow modeling, testing, and release hardening.

---

## Phase 1 - Make the current implementation compile and pass CI

### Goal

Ensure the current `main` branch builds cleanly before adding more features.

### Tasks

- Run local build verification:

```bash
cargo check --workspace --all-targets
cargo test --workspace --all-targets
```

- Fix any compiler errors from:
  - new commands
  - `reposcry-cache` DB additions
  - graph/review command changes
  - missing imports or trait bounds
  - enum/hash usage on `EdgeKind`

- Run the new full index command:

```bash
cargo install --path crates/reposcry-cli --force
reposcry --repo . index-full
```

- Run smoke tests:

```bash
reposcry --repo . get_architecture_overview --format json
reposcry --repo . query_graph "callers_of rebuild_graph"
reposcry --repo . query_graph "callees_of detect_changes"
reposcry --repo . get_impact_radius rebuild_graph --depth 4
```

### Acceptance criteria

- `cargo check --workspace --all-targets` passes.
- `cargo test --workspace --all-targets` passes.
- `reposcry --repo . index-full` produces JSON with successful steps.
- `reposcry get_architecture_overview` includes nonzero indexed data.
- CI is green on GitHub Actions.

---

## Phase 2 - Persist symbol-level call edges

### Current limitation

`reposcry warm-calls` persists call sites and file-level `Calls` edges. `reposcry` still reconstructs symbol-level call edges at runtime.

### Goal

Persist symbol-level call edges so `callers_of`, `callees_of`, and impact radius are faster and more stable.

### Tasks

- Add a `symbol_edges` table, or extend the existing `edges` table to support symbol IDs cleanly.

Recommended schema:

```sql
CREATE TABLE IF NOT EXISTS symbol_edges (
  id INTEGER PRIMARY KEY,
  source_symbol_id INTEGER NOT NULL,
  target_symbol_id INTEGER NOT NULL,
  source_file_id INTEGER NOT NULL,
  target_file_id INTEGER NOT NULL,
  kind TEXT NOT NULL,
  confidence REAL NOT NULL DEFAULT 1.0,
  metadata TEXT,
  FOREIGN KEY (source_file_id) REFERENCES files(id) ON DELETE CASCADE,
  FOREIGN KEY (target_file_id) REFERENCES files(id) ON DELETE CASCADE
);
```

- Store:
  - caller symbol
  - callee symbol
  - line number
  - confidence
  - resolution strategy: `same_file`, `unique_global`, `import_resolved`, etc.

- Add DB methods:
  - `insert_symbol_edges`
  - `get_symbol_edges_by_kind`
  - `symbol_edge_count`
  - `clear_symbol_edges_by_kind`

- Update `reposcry warm-calls` to persist both:
  - call-site rows
  - file-level call edges
  - symbol-level call edges

- Update `reposcry` to prefer persisted symbol-level call edges and only use runtime inference as fallback.

### Acceptance criteria

- `reposcry query_graph "callers_of X"` works without rescanning every source file.
- `get_architecture_overview` reports:
  - persisted call sites
  - persisted file call edges
  - persisted symbol call edges
- Runtime call inference can be disabled with a flag, for example:

```bash
reposcry query_graph "callers_of rebuild_graph" --no-runtime-calls
```

---

## Phase 3 - Replace lexical call scanning with AST call extraction

### Current limitation

Call extraction currently uses lexical line scanning for `foo(...)`, `obj.foo(...)`, and `module::foo(...)`-style expressions. This is useful but imprecise.

### Goal

Use Tree-sitter AST traversal to extract calls more accurately.

### Tasks

Implement AST call extraction per language:

#### Rust

Capture:

- `call_expression`
- `method_call_expression`
- macro invocation
- path expressions

Examples:

```rust
foo()
self.foo()
module::foo()
foo!()
```

#### TypeScript / JavaScript

Capture:

- `call_expression`
- member calls
- JSX component references
- React hooks

Examples:

```ts
foo()
obj.foo()
useThing()
<Component />
```

#### Python

Capture:

- `call`
- method calls
- decorators
- class construction

Examples:

```py
foo()
obj.foo()
@decorator
MyClass()
```

### Acceptance criteria

- Call extraction no longer depends mainly on line string scanning.
- Fewer false positives from function declarations, control flow, and comments.
- Calls inside multiline expressions are detected.
- Method calls are captured with useful metadata.

---

## Phase 4 - Add SQLite FTS5 search

### Current limitation

`semantic_search_nodes` is currently string-scored over loaded graph nodes.

### Goal

Add fast local full-text search before considering embeddings.

### Tasks

- Add FTS5 virtual table:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
  node_id,
  file_path,
  kind,
  name,
  signature,
  doc_comment,
  imports,
  content
);
```

- Populate during indexing or full indexing.
- Add DB method:

```rust
search_nodes_fts(query, kind, limit)
```

- Update `semantic_search_nodes`:
  - use FTS5 first
  - fallback to current string scoring
  - include score and match reason

### Acceptance criteria

```bash
reposcry semantic_search_nodes "cache database calls" --limit 20
```

returns higher-quality results than plain substring scoring.

---

## Phase 5 - Add optional embeddings / hybrid search

### Goal

Improve semantic search without making RepoScry dependent on cloud services.

### Tasks

- Keep embeddings optional.
- Support local embedding backends first:
  - Ollama
  - fastembed
  - candle-based model
- Add `--semantic` or config flag.
- Store vectors in:
  - SQLite blob table initially
  - optional external vector DB later if needed

### Acceptance criteria

- RepoScry works fully without embeddings.
- When embeddings are configured, `semantic_search_nodes` can use hybrid ranking:
  - FTS5 score
  - vector similarity
  - graph centrality
  - symbol kind boost

---

## Phase 6 - Stronger affected flow graph

### Current limitation

`get_affected_flows` detects entrypoint-like files heuristically.

### Goal

Model real execution flows from entrypoints to downstream calls and dependencies.

### Tasks

Add explicit flow detectors:

#### Next.js

- `app/**/page.tsx`
- `app/**/route.ts`
- `pages/api/**`
- server actions
- middleware
- layout-level providers

#### Rust

- `src/main.rs`
- `src/bin/**`
- Axum routes
- Actix routes
- worker loops
- queue consumers

#### Python

- FastAPI routes
- Flask routes
- Django views
- Celery tasks

### Output model

```json
{
  "entrypoint": "app/api/orders/route.ts",
  "kind": "nextjs_api_route",
  "path": [
    "POST",
    "validateOrder",
    "createOrder",
    "chargeStripe",
    "insertOrder"
  ],
  "changed_nodes": ["createOrder"],
  "risk": "high"
}
```

### Acceptance criteria

- `get_affected_flows` returns behavior-level paths, not only files.
- Flows include entrypoint, path, changed symbols, and confidence.

---

## Phase 7 - Better test intelligence

### Current limitation

Test suggestions are mostly filename heuristics.

### Goal

Suggest the minimum useful test set for a change.

### Tasks

- Create test mapping from:
  - imports from test files
  - call edges from test symbols
  - naming similarity
  - git co-change history
  - coverage files if available
- Add command:

```bash
reposcry query_graph "tests_for createOrder"
```

- Add suggested commands:
  - `cargo test module::test`
  - `pnpm vitest path`
  - `pytest path::test_name`

### Acceptance criteria

- `detect_changes` outputs precise suggested tests.
- Test suggestions include reason and confidence.
- Source change without test coverage is flagged more accurately.

---

## Phase 8 - Improve refactor planner

### Current limitation

`refactor_tool` is a dry-run heuristic.

### Goal

Make it useful enough to plan safe refactors.

### Tasks

Implement actions:

```bash
reposcry refactor_tool rename OldName NewName
reposcry refactor_tool dead-code
reposcry refactor_tool split-file path/to/file.rs
reposcry refactor_tool public-api-change main HEAD
```

For rename:

- definitions
- direct references
- dynamic-risk references
- import/export affected files
- test impact

For dead code:

- no callers
- not exported
- not route/entrypoint
- not public API
- not used by tests
- confidence levels

For split-file:

- symbols grouped by call/import clusters
- suggested module boundaries
- public exports to preserve

### Acceptance criteria

- Refactor plans include exact affected files and confidence.
- Dead-code candidates are grouped by confidence:
  - high
  - medium
  - low
  - public API risk

---

## Phase 9 - Harden MCP server

### Current limitation

The MCP server is minimal and line-oriented.

### Goal

Make it robust enough for real agent usage.

### Tasks

- Ensure protocol compatibility with target MCP clients.
- Add structured errors.
- Add request size limits.
- Add logging to stderr only.
- Add config file example:

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

- Add tests for:
  - initialize
  - tools/list
  - tools/call
  - invalid JSON
  - unknown tool

### Acceptance criteria

- Works from Claude Desktop / Cursor / compatible MCP client.
- No stdout noise except JSON-RPC responses.
- Tool calls return valid content blocks.

---

## Phase 10 - Performance and correctness benchmarks

### Goal

Prove RepoScry is faster/better instead of only claiming it.

### Tasks

Use `scripts/bench.sh` as the starting point.

Add benchmark outputs for:

- cold index time
- warm index time
- call warmup time
- architecture overview latency
- `detect_changes` latency
- `query_graph callers_of` latency
- `semantic_search_nodes` latency
- DB size
- number of files/symbols/imports/calls/edges

Add benchmark fixtures:

- small Rust repo
- medium Next.js repo
- mixed monorepo
- large repo

### Acceptance criteria

- A `BENCHMARKS.md` file exists.
- Results are reproducible.
- Each benchmark includes machine specs and repo size.
- Performance regressions can be detected.

---

## Phase 11 - Documentation and release polish

### Tasks

- Update main `README.md` with:
  - installation
  - update command
  - full-index workflow
  - CRG-compatible commands
  - MCP setup
  - examples
  - limitations
- Add `docs/architecture.md`.
- Add `docs/mcp.md`.
- Add `docs/benchmarks.md`.
- Add badges:
  - CI
  - crate version if published
  - license
- Decide package naming:
  - single installed CLI: `reposcry`
  - subcommands for indexing, call warmup, graph queries, MCP, and refactor planning

### Acceptance criteria

- A new user can install, index, run MCP, and query graph from README only.
- Limitations are clearly documented.
- Demo commands work after copy/paste.

---

## Phase 12 - Release pipeline

### Tasks

- Add GitHub release workflow.
- Build binaries for:
  - Linux x64
  - macOS ARM64
  - macOS x64
  - Windows x64
- Generate checksums.
- Optionally publish to crates.io.
- Add install script:

```bash
curl -fsSL https://raw.githubusercontent.com/zibouddd/resposcry/main/install.sh | bash
```

### Acceptance criteria

- Users can install without cloning the repo.
- Release binaries are attached to GitHub releases.
- Version command is consistent across binaries.

---

## Recommended immediate next steps

Do these in order:

1. Run:

```bash
cargo check --workspace --all-targets
```

2. Fix compile errors.
3. Run:

```bash
cargo test --workspace --all-targets
```

4. Run:

```bash
cargo install --path crates/reposcry-cli --force
reposcry --repo . index-full
```

5. Verify:

```bash
reposcry --repo . get_architecture_overview --format json
reposcry --repo . query_graph "callers_of rebuild_graph"
reposcry --repo . query_graph "callees_of detect_changes"
```

6. Only after build/test are green, continue with:
   - persistent symbol-level edges
   - AST call extraction
   - SQLite FTS5

---

## Definition of done

RepoScry can be considered better than the initial code-review-graph target when:

- It has CRG-compatible tools.
- It has MCP support.
- It has persisted file, symbol, import, call-site, and symbol-call graph data.
- `callers_of` and `callees_of` are function-level and fast.
- `get_affected_flows` returns real entrypoint-to-effect paths.
- `detect_changes` gives useful risk scores and tests.
- `semantic_search_nodes` uses FTS5 or hybrid search.
- CI is green.
- Benchmarks are published.
- README is enough for a new developer to use it without extra help.
