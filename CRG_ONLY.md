# CRG-only command policy

This package intentionally builds only the `crg` binary.

Removed:

- `graphify` compatibility binary alias
- installer documentation using `graphify ...` commands
- generated AI instruction references to the alias

Use:

```bash
crg install
crg install --platform codex
crg vscode install
crg cursor install
crg context "your task" --strict --budget 20000
```
