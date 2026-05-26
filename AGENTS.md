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
