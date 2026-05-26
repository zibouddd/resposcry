<!-- CRG:codex:BEGIN -->
# Code Review Graph instructions for Codex

Use RepoScry as the local repository map before editing code. The goal is to avoid blind edits and avoid sending the full repository into the model context.

## Mandatory workflow before editing

1. Make sure the repo is indexed:

```bash
reposcry index
```

2. Build a focused context pack for the current task:

```bash
reposcry context "$TASK" --strict --budget 20000 --format markdown > .reposcry/AI_CONTEXT.md
```

3. Read `.reposcry/AI_CONTEXT.md` before changing files.

4. For any file you plan to edit, inspect graph impact first:

```bash
reposcry explain path/to/file
reposcry deps path/to/file
reposcry rdeps path/to/file
```

5. After editing, validate the change:

```bash
reposcry validate main...HEAD
```

If the repository does not use `main`, replace `main...HEAD` with the correct base branch.

## Rules for the coding agent

- Do not load the entire repository when RepoScry can produce a smaller context pack.
- Do not edit a file only because its name looks relevant. Check dependencies and reverse dependencies first.
- If RepoScry reports LOW confidence, do not pretend the context is complete. Search for a better entrypoint or ask for one.
- Prefer `reposcry context`, `reposcry explain`, `reposcry deps`, and `reposcry rdeps` over broad file reads.
- Treat high fan-in files, API boundaries, database layers, event streams, and shared utilities as high-risk.
- Do not edit generated or vendor folders: `target/`, `.next/`, `node_modules/`, `dist/`, `build/`, `public/static/charting_library/`.
- Keep changes minimal and verify with tests or `reposcry validate`.

## Quick commands

```bash
reposcry stats
reposcry context "$TASK" --strict --budget 20000
reposcry report main...HEAD
reposcry rules check
reposcry validate main...HEAD
```
<!-- CRG:codex:END -->

<!-- reposcry:codex:BEGIN -->
# RepoScry instructions for Codex

Use RepoScry as the local repository map before editing code. The goal is to avoid blind edits and avoid sending the full repository into model context.

## Mandatory workflow before editing

1. Build or refresh a fast graph index without semantic vectors:

```bash
reposcry --repo . index --no-semantic
```

Do not run semantic/vector indexing in the normal edit loop. Semantic backends such as Candle/Qwen3 are intentionally opt-in because they can be slow on large repos.

2. Build a focused context pack for the current task:

```bash
reposcry --repo . context "$TASK" --strict --budget 20000 --format markdown > .reposcry/AI_CONTEXT.md
```

3. Read `.reposcry/AI_CONTEXT.md` before changing files.

4. For any file you plan to edit, inspect graph impact first:

```bash
reposcry --repo . explain path/to/file
reposcry --repo . deps path/to/file
reposcry --repo . rdeps path/to/file
```

5. After a batch of edits, update only changed files in the local graph/cache:

```bash
reposcry-update --repo . --changed --base main
```

If the repository does not use `main`, replace `main` with the correct base branch.

6. Review and validate the change:

```bash
reposcry --repo . detect_changes main HEAD --format json
reposcry --repo . validate main HEAD
```

## Rules for the coding agent

- Do not load the entire repository when RepoScry can produce a smaller context pack.
- Do not edit a file only because its name looks relevant. Check dependencies and reverse dependencies first.
- If RepoScry reports LOW confidence, do not pretend the context is complete. Search for a better entrypoint or ask for one.
- Prefer `reposcry context`, `reposcry explain`, `reposcry deps`, `reposcry rdeps`, and `reposcry query_graph` over broad file reads.
- After editing, prefer `reposcry-update --changed` over full `reposcry index`.
- Do not run semantic/vector refresh during normal editing.
- Treat high fan-in files, API boundaries, database layers, event streams, and shared utilities as high-risk.
- Do not edit generated or vendor folders: `target/`, `.next/`, `node_modules/`, `dist/`, `build/`, `public/static/charting_library/`.
- Keep changes minimal and verify with tests or `reposcry validate`.

## Quick commands

```bash
reposcry --repo . stats
reposcry --repo . context "$TASK" --strict --budget 20000
reposcry-update --repo . --changed --base main
reposcry --repo . detect_changes main HEAD --format json
reposcry --repo . report main HEAD
reposcry --repo . rules check
reposcry --repo . validate main HEAD
```

## Slow commands

Avoid these in the normal edit loop unless explicitly requested:

```bash
reposcry --repo . index
reposcry --repo . refresh-search --semantic-backend candle
```
<!-- reposcry:codex:END -->
