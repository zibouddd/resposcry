# CRG post-edit hook

After making code changes, the agent should run:

```bash
crg validate main...HEAD
crg report main...HEAD --format markdown > .code-review-graph/PR_REVIEW.md
```

If validation reports cycles, architecture violations, high-risk files without tests, or low-confidence context, fix or report it before claiming completion.
