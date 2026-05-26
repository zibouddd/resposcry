# Agent-Level Benchmarks

Tests how well RepoScry helps an AI coding agent navigate, analyze, and edit code.

## Files

| File | Purpose |
| ---- | ------- |
| `tasks.json` | 10 agent tasks (navigation, impact, bugfix, patch) |
| `run.py` | Automated runner for RepoScry context mode scoring |
| `RESULTS_TEMPLATE.json` | Template for full 3-mode manual results |

## Quick start (automated, RepoScry only)

```bash
# Run all 10 tasks through reposcry context
python benchmarks/agent/run.py --mode reposcry

# Run through reposcry get_review_context
python benchmarks/agent/run.py --mode reposcry_review

# Run a single task
python benchmarks/agent/run.py --mode reposcry --task nav_request_handling

# Save results (default: benchmarks/out/agent-bench-*.json)
```

## Full 3-mode benchmark (manual, requires LLM agent)

Compare 3 modes:

```txt
A. opencode baseline       — grep/read/shell only
B. opencode + CRG          — code-review-graph first
C. opencode + RepoScry     — reposcry first
```

For each mode and task:

1. Set system prompt per mode.
2. Run task prompt.
3. Record metrics in RESULTS_TEMPLATE.json.
4. Run each task 3× and average.
5. Calculate derived metrics (token reduction, speedup, accuracy).

## Scoring

0–5 per task, then averaged by mode.

| Score | Meaning                        |
| ----: | ------------------------------ |
|     0 | Wrong / failed                 |
|     1 | Unrelated files                |
|     2 | Some relevant info             |
|     3 | Correct area, incomplete       |
|     4 | Correct and useful             |
|     5 | Correct, concise, actionable   |

## Derived metrics

```txt
accuracy = avg(score) / 5
token_reduction = 1 - reposcry_tokens / baseline_tokens
file_read_reduction = 1 - reposcry_file_reads / baseline_file_reads
speedup = baseline_time / reposcry_time
```
