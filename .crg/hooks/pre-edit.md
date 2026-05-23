# CRG pre-edit hook

Before making code changes, the agent should run:

```bash
crg index
crg context "$TASK" --strict --budget 20000 --format markdown > .code-review-graph/AI_CONTEXT.md
```

Then read `.code-review-graph/AI_CONTEXT.md` and inspect dependencies for each planned edit file.
