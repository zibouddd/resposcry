# Code-review-graph compatibility

RepoScry exposes code-review-graph-style commands directly from `reposcry`.

Run `reposcry index` before using these commands.

## Commands

```bash
reposcry detect_changes main HEAD --format json
reposcry get_review_context "fix dependency graph rebuild" --budget 20000 --strict --format json
reposcry get_impact_radius crates/reposcry-cache/src/db.rs --depth 3 --format json
reposcry get_affected_flows main HEAD --format json
reposcry query_graph "imports_of crates/reposcry-cli/src/main.rs"
reposcry query_graph "imported_by crates/reposcry-cache/src/db.rs"
reposcry query_graph "symbols_in crates/reposcry-cli/src/main.rs"
reposcry query_graph "tests_for crates/reposcry-cache/src/db.rs"
reposcry semantic_search_nodes "cache db" --kind function --limit 20
reposcry semantic_search_nodes "cache db" --limit 20 --semantic
reposcry get_architecture_overview --format markdown
reposcry query_graph "callers_of rebuild_graph" --no-runtime-calls
reposcry refactor_tool dead-code
reposcry refactor_tool rename CacheDb RepoCacheDb
reposcry refactor_tool split-file crates/reposcry-cli/src/main.rs
```

Kebab-case aliases are also available for the CRG command names:

```bash
reposcry detect-changes main HEAD
reposcry get-review-context "task"
reposcry get-impact-radius CacheDb
reposcry get-affected-flows main HEAD
reposcry query-graph "imports_of src/main.rs"
reposcry semantic-search-nodes "auth route"
reposcry get-architecture-overview
reposcry refactor-tool dead-code
```

## Notes

The compatibility layer reuses RepoScry's existing local SQLite cache at `.reposcry/reposcry.db`.

Current output is based on indexed files, symbols, imports, persisted call sites, persisted symbol-level call edges, and resolved import edges. Runtime call inference remains available as a fallback when persisted call edges are missing.
