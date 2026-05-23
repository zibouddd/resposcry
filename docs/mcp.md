# RepoScry MCP

RepoScry exposes a minimal MCP-compatible stdio server from the `reposcry` CLI:

```bash
reposcry mcp --repo /path/to/repo
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

## Example client config

```json
{
  "mcpServers": {
    "reposcry": {
      "command": "reposcry",
      "args": ["mcp", "--repo", "/path/to/repo"]
    }
  }
}
```

## Notes

- Requests are newline-delimited JSON-RPC messages over stdio.
- Server logs are written to stderr only.
- Successful tool calls return MCP text content blocks containing formatted JSON.
- Oversized requests and invalid JSON return structured JSON-RPC errors.
