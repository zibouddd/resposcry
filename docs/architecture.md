# RepoScry Architecture

RepoScry is a local code review graph engine with four main layers.

## CLI layer

- `reposcry`: repository indexing, context generation, validation, reporting, and install helpers
- `reposcry-crg`: CRG-compatible analysis commands plus MCP stdio server
- `reposcry-index-full`: convenience wrapper for full indexing flows

## Storage layer

SQLite database under `.reposcry/reposcry.db` stores:

- files
- symbols
- imports
- file-level edges
- call sites
- symbol-level call edges
- search index rows

## Analysis layer

- Tree-sitter parsers extract symbols, imports, calls, and tests
- dependency resolvers rebuild file-level import edges
- call-edge rebuilders persist file and symbol call graphs
- CRG-compatible queries expose impact, search, and refactor planning

## Output layer

- Markdown and JSON CLI output
- MCP `tools/list` and `tools/call`
- benchmark JSON snapshots
- AI context packs in `.reposcry/AI_CONTEXT.md`
