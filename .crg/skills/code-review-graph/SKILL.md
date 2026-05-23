# Skill: Code Review Graph assisted coding

Use this skill when you are asked to change, review, refactor, debug, or explain a repository.

## Purpose

CRG is a local code graph and AI context compiler. It gives the agent a compact map of relevant files, dependencies, reverse dependencies, symbols, tests, risk warnings, and architecture rules.

## Required behavior

Before editing code:

```bash
crg index
crg context "$TASK" --strict --budget 20000 --format markdown > .code-review-graph/AI_CONTEXT.md
```

Read `.code-review-graph/AI_CONTEXT.md`. Then inspect planned edit files:

```bash
crg explain <file>
crg deps <file>
crg rdeps <file>
```

After editing:

```bash
crg validate main...HEAD
```

## Anti-token-bloat rule

Never paste the full repository or full graph into context. Ask CRG for the smallest useful context pack.
