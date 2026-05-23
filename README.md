# Code Review Graph (CRG)

Rust CLI that indexes a repository, builds a local dependency graph, and generates compact AI context packs so coding agents do not edit blindly.

## Basic usage

```bash
cargo install --path crates/reposcry-cli --force
reposcry-cli -- init
reposcry-cli -- index
reposcry-cli -- stats
reposcry-cli -- deps crates/reposcry-cli/src/main.rs
reposcry-cli -- rdeps crates/reposcry-cache/src/db.rs
reposcry-cli -- context "fix dependency graph rebuild" --budget

OR
cargo run -p reposcry-cli -- init
cargo run -p reposcry-cli -- index
cargo run -p reposcry-cli -- stats
cargo run -p reposcry-cli -- deps crates/reposcry-cli/src/main.rs
cargo run -p reposcry-cli -- rdeps crates/reposcry-cache/src/db.rs
cargo run -p reposcry-cli -- context "fix dependency graph rebuild" --budget 20000 --strict
```

## What is indexed

- files
- symbols
- imports
- resolved file-level import edges
- git diff impact, when inside a Git repo

The full graph stays local in `.code-review-graph/reposcry.db`. AI receives only the selected context pack.

## Important fix in this version

Imports are now persisted in SQLite and rebuilt into `Imports` edges. This makes `deps`, `rdeps`, `diff`, `rules check`, and `context` useful for impact analysis.

## AI agent / IDE installers

CRG can install repository instructions, skills, and hooks for common coding agents:

```bash
reposcry install                         # Claude Code Linux/Mac
reposcry install --platform windows      # Claude Code Windows
reposcry install --platform codex
reposcry install --platform opencode
reposcry install --platform copilot
reposcry vscode install
reposcry install --platform aider
reposcry install --platform claw
reposcry install --platform droid
reposcry install --platform trae
reposcry install --platform trae-cn
reposcry install --platform gemini
reposcry install --platform hermes
reposcry install --platform kimi
reposcry kiro install
reposcry install --platform pi
reposcry cursor install
reposcry antigravity install
reposcry hooks install
```

Use `--dry-run` to preview and `--force` to overwrite existing non-managed files.

The installer creates `.reposcry/skills/code-review-graph/SKILL.md`, platform instruction files, and helper scripts so agents run `reposcry context` before editing and `reposcry validate` after editing.

Only the `reposcry` binary is built. No `graphify` alias is generated.
