# Scoring v2 summary

Task: `add a new cache backend to reposcry-cache crate`

| Mode | Tokens | Legacy | Final | Score/1k v2 | File recall | Symbol recall | Tests | Noise |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| reposcry_context_budget_4000 | 473 | None | 0.25 | 0.529 | 0.0 | 0.0 | 0.0 | 0.0 |
| reposcry_context_budget_16000 | 521 | None | 0.25 | 0.48 | 0.0 | 0.0 | 0.0 | 0.0 |
| reposcry_context_budget_8000 | 528 | None | 0.25 | 0.473 | 0.0 | 0.0 | 0.0 | 0.0 |
| reposcry_context_budget_20000 | 533 | None | 0.25 | 0.469 | 0.0 | 0.0 | 0.0 | 0.0 |
| reposcry_review_context_budget_4000 | 606 | None | 0.25 | 0.413 | 0.0 | 0.0 | 0.0 | 0.0 |
| reposcry_review_context_budget_8000 | 679 | None | 0.25 | 0.368 | 0.0 | 0.0 | 0.0 | 0.0 |
| reposcry_review_context_budget_20000 | 773 | None | 0.25 | 0.323 | 0.0 | 0.0 | 0.0 | 0.0 |
| reposcry_review_context_budget_16000 | 812 | None | 0.25 | 0.308 | 0.0 | 0.0 | 0.0 | 0.0 |
| crg_mcp_raw | 1407 | None | 0.25 | 0.178 | 0.0 | 0.0 | 0.0 | 0.0 |
| crg_mcp_retried | 2097 | None | 0.25 | 0.119 | 0.0 | 0.0 | 0.0 | 0.0 |
| crg_manual_best | 3089 | None | 0.225 | 0.073 | 0.0 | 0.0 | 0.0 | 0.0 |
| crg_wiki | 12353 | None | 0.113 | 0.009 | 0.0 | 0.0 | 0.0 | 0.0 |

## Acceptance gates

The compact RepoScry mode should pass:

```text
reposcry_context_budget_4000 final_score >= 4.5
reposcry_context_budget_4000 score_per_1k_tokens_v2 > best CRG score_per_1k_tokens_v2
duplicate_path_count == 0
encoding_error_count == 0
irrelevant_rule_count == 0
includes_manifest == true
includes_expected_test == true
```
