$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

& (Join-Path $PSScriptRoot "setup-benchmark-fixtures.ps1") -FixtureName "small_rust_repo"

$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("reposcry-readme-smoke-" + [System.Guid]::NewGuid().ToString("N"))
$installRoot = Join-Path $tmpRoot "install-root"
$fixtureRepo = Resolve-Path (Join-Path $Root "benchmarks/fixtures/small_rust_repo")

function Assert-True {
    param(
        [Parameter(Mandatory = $true)]
        [bool]$Condition,
        [Parameter(Mandatory = $true)]
        [string]$Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

try {
    cargo install --path crates/reposcry-cli --force --offline --root $installRoot | Out-Null
    $reposcry = Join-Path $installRoot "bin/reposcry.exe"
    Assert-True (Test-Path $reposcry) "Installed reposcry.exe was not found"

    & $reposcry --repo $fixtureRepo init | Out-Null
    & $reposcry --repo $fixtureRepo index --no-semantic | Out-Null
    & $reposcry --repo $fixtureRepo warm-calls | Out-Null
    & $reposcry --repo $fixtureRepo refresh-search --semantic-backend local-hash-v1 | Out-Null

    $arch = (& $reposcry --repo $fixtureRepo get_architecture_overview --format json) | ConvertFrom-Json
    Assert-True ($arch.files_indexed -gt 0) "README smoke: expected indexed files"
    Assert-True ($arch.persisted_symbol_call_edges -gt 0) "README smoke: expected persisted symbol call edges"

    $query = (& $reposcry --repo $fixtureRepo query_graph "callers_of rebuild_graph" --no-runtime-calls) | ConvertFrom-Json
    Assert-True ($query.edges.Count -gt 0) "README smoke: expected callers_of rebuild_graph edges"

    $search = (& $reposcry --repo $fixtureRepo semantic_search_nodes "graph cache rebuild" --limit 5 --semantic --semantic-backend local-hash-v1) | ConvertFrom-Json
    Assert-True ($search.hits.Count -gt 0) "README smoke: expected semantic search results"

    $initialize = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0.0.0"}}}'
    $mcp = ($initialize | & $reposcry mcp --repo $fixtureRepo) | ConvertFrom-Json
    Assert-True ($mcp.result.serverInfo.name -eq "reposcry") "README smoke: MCP initialize returned unexpected server name"

    & $reposcry --repo $fixtureRepo validate main | Out-Null
    Write-Output "README smoke passed"
} finally {
    if (Test-Path $tmpRoot) {
        Remove-Item -Recurse -Force $tmpRoot
    }
}
