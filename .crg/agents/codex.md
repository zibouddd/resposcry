# Code Review Graph instructions for Codex

Use CRG as the local repository map before editing code. The goal is to avoid blind edits and avoid sending the full repository into the model context.

## Mandatory workflow before editing

1. Make sure the repo is indexed:

```bash
crg index
```

2. Build a focused context pack for the current task:

```bash
crg context "$TASK" --strict --budget 20000 --format markdown > .code-review-graph/AI_CONTEXT.md
```

3. Read `.code-review-graph/AI_CONTEXT.md` before changing files.

4. For any file you plan to edit, inspect graph impact first:

```bash
crg explain path/to/file
crg deps path/to/file
crg rdeps path/to/file
```

5. After editing, validate the change:

```bash
crg validate main...HEAD
```

If the repository does not use `main`, replace `main...HEAD` with the correct base branch.

## Rules for the coding agent

- Do not load the entire repository when CRG can produce a smaller context pack.
- Do not edit a file only because its name looks relevant. Check dependencies and reverse dependencies first.
- If CRG reports LOW confidence, do not pretend the context is complete. Search for a better entrypoint or ask for one.
- Prefer `crg context`, `crg explain`, `crg deps`, and `crg rdeps` over broad file reads.
- Treat high fan-in files, API boundaries, database layers, event streams, and shared utilities as high-risk.
- Do not edit generated or vendor folders: `target/`, `.next/`, `node_modules/`, `dist/`, `build/`, `public/static/charting_library/`.
- Keep changes minimal and verify with tests or `crg validate`.

## Quick commands

```bash
crg stats
crg context "$TASK" --strict --budget 20000
crg report main...HEAD
crg rules check
crg validate main...HEAD
```
