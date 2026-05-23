# Fixes applied

This patched version focuses on making the dependency graph actually usable for AI context generation.

## Critical fixes

1. **Persist parsed imports**
   - Added `CacheDb::insert_imports`.
   - Added `CachedImport` and import retrieval APIs.
   - Stores `imported_names` as JSON.

2. **Persist resolved import edges**
   - Added `edges` persistence helpers.
   - `reposcry index` now resolves imports after scanning and stores `Imports` edges.

3. **Rebuild graph correctly from SQLite**
   - `rebuild_graph` now rebuilds:
     - file nodes
     - symbol nodes
     - `Contains` edges
     - `Imports` edges
   - Fixed the previous DB ID vs graph node ID confusion.

4. **Fix dependency commands**
   - `deps`, `rdeps`, `diff`, `explain`, `context`, and `report` now use only `Imports` edges for dependency impact.
   - This avoids treating `file -> symbol` `Contains` edges as dependencies.

5. **Fix SQLite upsert ID bug**
   - `upsert_file` no longer trusts `last_insert_rowid()` after `ON CONFLICT DO UPDATE`.
   - It fetches the file by path after the upsert.

6. **Remove committed graph cache**
   - Deleted `.code-review-graph/reposcry.db` from the repo.
   - Added `.gitignore` rules for `.code-review-graph/` and local DB files.

7. **Improve import resolution**
   - Relative TypeScript imports: `./x`, `../x`.
   - Alias imports: `@/x`, `~/x`.
   - Rust workspace imports: `crg_graph::edge::EdgeKind` -> `crates/reposcry-graph/src/edge.rs`.
   - Rust local imports: `crate::`, `self::`, `super::`.

8. **Improve AI context pack quality**
   - Deduplicates matched files.
   - Uses a rough token-budget cap.
   - Uses only import edges for dependency paths and reverse dependencies.
   - Excludes file nodes from symbol summaries.

## Note

I could not run `cargo test` in this sandbox because Rust/Cargo is not installed here. The patch is structured to compile, but run the commands below locally:

```bash
cargo fmt
cargo test
cargo run -p reposcry-cli -- init
cargo run -p reposcry-cli -- index
cargo run -p reposcry-cli -- stats
```
