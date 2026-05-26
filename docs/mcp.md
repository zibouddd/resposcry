# RepoScry MCP

RepoScry exposes two MCP-compatible stdio entrypoints.

## Main CRG-compatible server

```bash
reposcry --repo /path/to/repo mcp
```

Supported MCP methods:

- `initialize`
- `tools/list`
- `tools/call`

Supported tools:

- `detect_changes`
- `get_review_context`
- `get_impact_radius`
- `get_affected_flows`
- `query_graph`
- `semantic_search_nodes`
- `get_architecture_overview`
- `refactor_tool`

## MCP-plus server

`reposcry-mcp-plus` is a fuller read-only graph inspection server:

```bash
reposcry-mcp-plus --repo /path/to/repo
```

Supported tools:

- `get_graph_summary`
- `list_languages`
- `list_files`
- `list_symbols`
- `get_file_neighborhood`
- `export_graph_json`

Example JSON-RPC smoke test:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_graph_summary","arguments":{}}}' \
  | reposcry-mcp-plus --repo .
```

## Example client config

```json
{
  "mcpServers": {
    "reposcry": {
      "command": "reposcry",
      "args": ["--repo", "/path/to/repo", "mcp"]
    },
    "reposcry-plus": {
      "command": "reposcry-mcp-plus",
      "args": ["--repo", "/path/to/repo"]
    }
  }
}
```

## Notes

- Requests are newline-delimited JSON-RPC messages over stdio.
- Server logs are written to stderr only.
- Successful tool calls return MCP text content blocks containing formatted JSON.
- Oversized requests and invalid JSON return structured JSON-RPC errors.
