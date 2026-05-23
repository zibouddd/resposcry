$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$OutDir = if ($env:BENCH_OUT_DIR) { $env:BENCH_OUT_DIR } else { "benchmarks/out" }
$OutFile = Join-Path $OutDir "latest.json"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

cargo build -p reposcry-cli --bins | Out-Null
$ReposcryBin = if ($env:REPOSCRY_BIN) { $env:REPOSCRY_BIN } else { Join-Path $Root "target/debug/reposcry.exe" }
$CrgBin = if ($env:CRG_BIN) { $env:CRG_BIN } else { Join-Path $Root "target/debug/reposcry-crg.exe" }

function Measure-Millis([scriptblock]$Action) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $Action
    $sw.Stop()
    return [int64]$sw.ElapsedMilliseconds
}

$coldIndexMs = Measure-Millis { & $ReposcryBin --repo . index | Out-Null }
$warmIndexMs = Measure-Millis { & $ReposcryBin --repo . index | Out-Null }
$archMs = Measure-Millis { & $CrgBin --repo . get_architecture_overview --format json | Out-Null }
$callersMs = Measure-Millis { & $CrgBin --repo . query_graph "callers_of rebuild_graph" | Out-Null }
$searchMs = Measure-Millis { & $CrgBin --repo . semantic_search_nodes "cache database calls" --limit 20 | Out-Null }

$archJson = & $CrgBin --repo . get_architecture_overview --format json
$arch = $archJson | ConvertFrom-Json
$osLine = ((cmd /c ver) | Where-Object { $_.Trim() -ne "" } | Select-Object -Last 1).Trim()

$dbSize = 0
if (Test-Path ".reposcry/reposcry.db") {
    $dbSize = (Get-Item ".reposcry/reposcry.db").Length
}

$payload = [ordered]@{
    captured_at = [DateTime]::UtcNow.ToString("o")
    machine = [ordered]@{
        os = $osLine
        cpu = $env:PROCESSOR_IDENTIFIER
        memory_gb = if ($env:REPOSCRY_MEMORY_GB) { $env:REPOSCRY_MEMORY_GB } else { "unknown" }
    }
    repo = [ordered]@{
        path = $Root.Path
        fixture = "current_repo"
    }
    metrics = [ordered]@{
        cold_index_ms = $coldIndexMs
        warm_index_ms = $warmIndexMs
        architecture_overview_ms = $archMs
        query_graph_callers_ms = $callersMs
        semantic_search_ms = $searchMs
        db_size_bytes = $dbSize
        files_indexed = $arch.files_indexed
        symbols_indexed = $arch.symbols_indexed
        imports_indexed = $arch.imports_indexed
        persisted_call_sites = $arch.persisted_call_sites
        persisted_symbol_call_edges = $arch.persisted_symbol_call_edges
        persisted_file_call_edges = $arch.persisted_file_call_edges
    }
}

$payload | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $OutFile
Get-Content $OutFile
