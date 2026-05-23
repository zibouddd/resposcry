# Code Review Graph (CRG)

Rust CLI that indexes a repository, builds a local dependency graph, and generates compact AI context packs so coding agents do not edit blindly.

## Basic usage

```bash
cargo install --path crates/crg-cli --force
cargo run -p crg-cli -- init
cargo run -p crg-cli -- index
cargo run -p crg-cli -- stats
cargo run -p crg-cli -- deps crates/crg-cli/src/main.rs
cargo run -p crg-cli -- rdeps crates/crg-cache/src/db.rs
cargo run -p crg-cli -- context "fix dependency graph rebuild" --budget 20000 --strict
```

## What is indexed

- files
- symbols
- imports
- resolved file-level import edges
- git diff impact, when inside a Git repo

The full graph stays local in `.code-review-graph/crg.db`. AI receives only the selected context pack.

## Important fix in this version

Imports are now persisted in SQLite and rebuilt into `Imports` edges. This makes `deps`, `rdeps`, `diff`, `rules check`, and `context` useful for impact analysis.

## AI agent / IDE installers

CRG can install repository instructions, skills, and hooks for common coding agents:

```bash
crg install                         # Claude Code Linux/Mac
crg install --platform windows      # Claude Code Windows
crg install --platform codex
crg install --platform opencode
crg install --platform copilot
crg vscode install
crg install --platform aider
crg install --platform claw
crg install --platform droid
crg install --platform trae
crg install --platform trae-cn
crg install --platform gemini
crg install --platform hermes
crg install --platform kimi
crg kiro install
crg install --platform pi
crg cursor install
crg antigravity install
crg hooks install
```

Use `--dry-run` to preview and `--force` to overwrite existing non-managed files.

The installer creates `.crg/skills/code-review-graph/SKILL.md`, platform instruction files, and helper scripts so agents run `crg context` before editing and `crg validate` after editing.

Only the `crg` binary is built. No `graphify` alias is generated.
