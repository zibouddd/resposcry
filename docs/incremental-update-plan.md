# RepoScry incremental update plan

This document defines the optimized indexing workflow for AI agents and large repositories.

## Problem

A full `reposcry index` is too expensive to run after every AI edit when the repository has thousands of files.

The current full index does several expensive phases:

1. scan repository files
2. hash files
3. parse changed files
4. persist symbols/imports/call sites
5. rebuild import edges
6. rebuild call edges
7. rebuild search documents
8. optionally rebuild semantic vectors

For large repositories, semantic vectors and global rebuilds can dominate runtime.

## Goal

After an AI agent edits a small set of files, update only the affected graph/cache data.

Target workflow:

```bash
reposcry-update --repo . --changed
```

Future CLI wrapper target:

```bash
reposcry --repo . update --changed
```

## Implemented command

The first implementation is a dedicated binary:

```bash
reposcry-update --repo . --changed
```

It can also update explicit files:

```bash
reposcry-update --repo . --file src/main.rs --file crates/foo/src/lib.rs
```

And it can compare against a base ref:

```bash
reposcry-update --repo . --changed --base main
```

## Current behavior

`reposcry-update --changed`:

1. collects files from `git status --porcelain`
2. collects files from `git diff --name-only <base>`
3. skips ignored/generated/binary-looking paths
4. reparses only changed source files
5. deletes DB rows for deleted files through `CacheDb::delete_file`
6. updates file rows, symbols, imports, and call sites
7. rebuilds persisted import edges
8. runs `reposcry warm-calls` unless `--skip-warm-calls` is passed
9. optionally runs `reposcry refresh-search --no-semantic` when `--refresh-search` is passed

Semantic vectors are not refreshed by default.

## Recommended AI-agent workflow

Before code exploration:

```bash
reposcry --repo . index --no-semantic
```

After a batch of AI edits:

```bash
reposcry-update --repo . --changed --base main
reposcry --repo . detect_changes main HEAD
reposcry --repo . validate main HEAD
```

For very fast inner-loop updates:

```bash
reposcry-update --repo . --changed --skip-warm-calls
```

For updated lexical search documents:

```bash
reposcry-update --repo . --changed --refresh-search
```

Do not run semantic vector refresh in the edit loop.

## Intended future `reposcry update` integration

The dedicated binary should eventually be exposed directly through the main CLI:

```bash
reposcry --repo . update --changed
reposcry --repo . update --file src/main.rs
reposcry --repo . update --changed --base main --refresh-search
```

Recommended enum shape:

```rust
Commands::Update {
    changed: bool,
    files: Vec<String>,
    base: String,
    skip_warm_calls: bool,
    refresh_search: bool,
}
```

The implementation should call the same incremental update logic used by `reposcry-update`.

## Optimization roadmap

### Phase 1: Changed-file updater

Status: implemented as `reposcry-update`.

Acceptance criteria:

```bash
reposcry-update --repo . --changed
```

returns JSON with:

- files seen
- parsed files
- deleted files
- skipped files
- errors
- rebuilt import edge count
- whether call warmup ran

### Phase 2: Direct main CLI integration

Add:

```bash
reposcry --repo . update --changed
```

without breaking existing `reposcry index` behavior.

### Phase 3: Affected-edge rebuild

Current implementation rebuilds all import edges after changed-file parsing.

Optimize to:

- delete outgoing edges from changed files
- rebuild outgoing edges for changed files
- re-resolve direct reverse dependents only when imports change
- handle deleted/renamed files cleanly

Target speed for 1-10 changed files in 3k-file repo: under 1-5 seconds.

### Phase 4: Incremental search index

Current `--refresh-search` uses the existing global refresh path.

Optimize to:

- delete search documents for changed/deleted files
- insert search documents for changed files only
- keep semantic vectors untouched unless explicitly requested

### Phase 5: Watch mode

Add:

```bash
reposcry --repo . watch --no-semantic
```

Behavior:

- debounce file changes
- update changed files in batches
- keep graph cache warm for AI agents

### Phase 6: Agent hooks

Add installer-generated scripts:

```bash
.reposcry/hooks/ai-preflight.sh
.reposcry/hooks/ai-postedit.sh
```

Preflight:

```bash
reposcry --repo . index --no-semantic
```

Post-edit:

```bash
reposcry-update --repo . --changed --base main
reposcry --repo . validate main HEAD
```

## Performance target

For a 3k-file repository:

| Operation | Target |
|---|---:|
| Full index without semantic vectors | 1-5 min |
| Incremental update, 1 changed file | <1 sec |
| Incremental update, 10 changed files | <5 sec |
| Incremental update with warm calls | <10 sec |
| Semantic vector refresh with Candle/Qwen3 | separate, opt-in, slow |

## Important rule

The AI agent should not directly edit the SQLite graph.

Correct architecture:

```text
AI edits files
RepoScry detects changed files
RepoScry reparses changed files
RepoScry updates DB deterministically
AI queries graph again
```
