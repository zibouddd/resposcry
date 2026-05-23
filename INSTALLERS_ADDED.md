# CRG installer layer added

This version adds `reposcry install` commands so AI coding agents can discover and use CRG before editing. The package builds only one binary: `reposcry`.

## Commands

| Platform              | Install command                        |
| --------------------- | -------------------------------------- |
| Claude Code Linux/Mac | `reposcry install`                     |
| Claude Code Windows   | `reposcry install --platform windows`  |
| Codex                 | `reposcry install --platform codex`    |
| OpenCode              | `reposcry install --platform opencode` |
| GitHub Copilot CLI    | `reposcry install --platform copilot`  |
| VS Code Copilot Chat  | `reposcry vscode install`              |
| Aider                 | `reposcry install --platform aider`    |
| OpenClaw              | `reposcry install --platform claw`     |
| Factory Droid         | `reposcry install --platform droid`    |
| Trae                  | `reposcry install --platform trae`     |
| Trae CN               | `reposcry install --platform trae-cn`  |
| Gemini CLI            | `reposcry install --platform gemini`   |
| Hermes                | `reposcry install --platform hermes`   |
| Kimi Code             | `reposcry install --platform kimi`     |
| Kiro IDE/CLI          | `reposcry kiro install`                |
| Pi coding agent       | `reposcry install --platform pi`       |
| Cursor                | `reposcry cursor install`              |
| Google Antigravity    | `reposcry antigravity install`         |
| Hook scripts only     | `reposcry hooks install`               |
| Everything            | `reposcry install --platform all`      |

Add `--dry-run` to preview writes and `--force` to overwrite existing non-managed files.

## What gets installed

- `.reposcry/skills/code-review-graph/SKILL.md`
- `.reposcry/agents/*.md`
- `.reposcry/hooks/pre-edit.md`
- `.reposcry/hooks/post-edit.md`
- `scripts/reposcry-context.sh`
- `scripts/reposcry-validate.sh`
- `scripts/reposcry-context.ps1`
- `scripts/reposcry-validate.ps1`
- platform-specific instruction files such as `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, `.cursor/rules/reposcry-context.mdc`, `.github/copilot-instructions.md`, or `.kiro/steering/reposcry.md`

The installer uses CRG-managed marker blocks for shared instruction files where possible, so repeated installs are idempotent.

## Agent workflow installed

```bash
reposcry index
reposcry context "$TASK" --strict --budget 20000 --format markdown > .code-review-graph/AI_CONTEXT.md
reposcry explain <file>
reposcry deps <file>
reposcry rdeps <file>
reposcry validate main...HEAD
```

The goal is to force the AI to use a small context pack instead of reading the whole repository.
