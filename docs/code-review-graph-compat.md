# Code-review-graph compatibility

RepoScry ships a `reposcry-crg` binary for agent workflows that expect code-review-graph-style commands.

Run `reposcry index` before using these commands.

## Commands

```bash
reposcry-crg detect_changes main HEAD --format json
reposcry-crg get_review_context "fix dependency graph rebuild" --budget 20000 --strict --format json
reposcry-crg get_impact_radius crates/reposcry-cache/src/db.rs --depth 3 --format json
reposcry-crg get_affected_flows main HEAD --format json
reposcry-crg query_graph "imports_of crates/reposcry-cli/src/main.rs"
reposcry-crg query_graph "imported_by crates/reposcry-cache/src/db.rs"
reposcry-crg query_graph "symbols_in crates/reposcry-cli/src/main.rs"
reposcry-crg query_graph "tests_for crates/reposcry-cache/src/db.rs"
reposcry-crg semantic_search_nodes "cache db" --kind function --limit 20
reposcry-crg get_architecture_overview --format markdown
reposcry-crg refactor_tool dead-code
reposcry-crg refactor_tool rename CacheDb RepoCacheDb
reposcry-crg refactor_tool split-file crates/reposcry-cli/src/main.rs
```

Kebab-case aliases are also available for the CRG command names:

```bash
reposcry-crg detect-changes main HEAD
reposcry-crg get-review-context "task"
reposcry-crg get-impact-radius CacheDb
reposcry-crg get-affected-flows main HEAD
reposcry-crg query-graph "imports_of src/main.rs"
reposcry-crg semantic-search-nodes "auth route"
reposcry-crg get-architecture-overview
reposcry-crg refactor-tool dead-code
```

## Notes

The compatibility layer reuses RepoScry's existing local SQLite cache at `.reposcry/reposcry.db`.

Current output is based on indexed files, symbols, imports, and resolved import edges. Precise function-level call-flow output requires future call-edge indexing.
