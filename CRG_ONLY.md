# CRG-only command policy

This package intentionally builds only the `reposcry` binary.

Removed:

- `graphify` compatibility binary alias
- installer documentation using `graphify ...` commands
- generated AI instruction references to the alias

Use:

```bash
reposcry install
reposcry install --platform codex
reposcry vscode install
reposcry cursor install
reposcry context "your task" --strict --budget 20000
```
