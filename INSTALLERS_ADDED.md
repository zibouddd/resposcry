# CRG installer layer added

This version adds `crg install` commands so AI coding agents can discover and use CRG before editing. The package builds only one binary: `crg`.

## Commands

| Platform | Install command |
|---|---|
| Claude Code Linux/Mac | `crg install` |
| Claude Code Windows | `crg install --platform windows` |
| Codex | `crg install --platform codex` |
| OpenCode | `crg install --platform opencode` |
| GitHub Copilot CLI | `crg install --platform copilot` |
| VS Code Copilot Chat | `crg vscode install` |
| Aider | `crg install --platform aider` |
| OpenClaw | `crg install --platform claw` |
| Factory Droid | `crg install --platform droid` |
| Trae | `crg install --platform trae` |
| Trae CN | `crg install --platform trae-cn` |
| Gemini CLI | `crg install --platform gemini` |
| Hermes | `crg install --platform hermes` |
| Kimi Code | `crg install --platform kimi` |
| Kiro IDE/CLI | `crg kiro install` |
| Pi coding agent | `crg install --platform pi` |
| Cursor | `crg cursor install` |
| Google Antigravity | `crg antigravity install` |
| Hook scripts only | `crg hooks install` |
| Everything | `crg install --platform all` |

Add `--dry-run` to preview writes and `--force` to overwrite existing non-managed files.

## What gets installed

- `.crg/skills/code-review-graph/SKILL.md`
- `.crg/agents/*.md`
- `.crg/hooks/pre-edit.md`
- `.crg/hooks/post-edit.md`
- `scripts/crg-context.sh`
- `scripts/crg-validate.sh`
- `scripts/crg-context.ps1`
- `scripts/crg-validate.ps1`
- platform-specific instruction files such as `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, `.cursor/rules/crg-context.mdc`, `.github/copilot-instructions.md`, or `.kiro/steering/crg.md`

The installer uses CRG-managed marker blocks for shared instruction files where possible, so repeated installs are idempotent.

## Agent workflow installed

```bash
crg index
crg context "$TASK" --strict --budget 20000 --format markdown > .code-review-graph/AI_CONTEXT.md
crg explain <file>
crg deps <file>
crg rdeps <file>
crg validate main...HEAD
```

The goal is to force the AI to use a small context pack instead of reading the whole repository.
