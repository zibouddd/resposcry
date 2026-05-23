$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$ManifestPath = Join-Path $Root "benchmarks/fixtures.json"
$FixtureName = if ($env:REPOSCRY_BENCH_FIXTURE) { $env:REPOSCRY_BENCH_FIXTURE } else { "current_repo" }
$Fixtures = (Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json).fixtures
$Fixture = $Fixtures | Where-Object { $_.name -eq $FixtureName } | Select-Object -First 1
if (-not $Fixture) {
    throw "Unknown benchmark fixture: $FixtureName"
}

if ($Fixture.name -ne "current_repo") {
    & (Join-Path $PSScriptRoot "setup-benchmark-fixtures.ps1") -FixtureName $Fixture.name
}

$FixtureRepo = Resolve-Path (Join-Path $Root $Fixture.path)
$CallersQuery = if ($Fixture.callers_query) { $Fixture.callers_query } else { "callers_of rebuild_graph" }
$SemanticQuery = if ($Fixture.semantic_query) { $Fixture.semantic_query } else { "cache database calls" }

$OutDir = if ($env:BENCH_OUT_DIR) { $env:BENCH_OUT_DIR } else { "benchmarks/out" }
$OutName = if ($env:BENCH_OUT_NAME) {
    $env:BENCH_OUT_NAME
} elseif ($env:REPOSCRY_BENCH_FIXTURE -and $env:REPOSCRY_BENCH_SEMANTIC_BACKEND) {
    "latest-$FixtureName-$($env:REPOSCRY_BENCH_SEMANTIC_BACKEND).json"
} elseif ($env:REPOSCRY_BENCH_FIXTURE) {
    "latest-$FixtureName.json"
} elseif ($env:REPOSCRY_BENCH_SEMANTIC_BACKEND) {
    "latest-$($env:REPOSCRY_BENCH_SEMANTIC_BACKEND).json"
} else {
    "latest.json"
}
$OutFile = Join-Path $OutDir $OutName
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

cargo build -p reposcry-cli --bins | Out-Null
$ReposcryBin = if ($env:REPOSCRY_BIN) { $env:REPOSCRY_BIN } else { Join-Path $Root "target/debug/reposcry.exe" }
$SemanticBenchBackend = $env:REPOSCRY_BENCH_SEMANTIC_BACKEND

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [scriptblock]$Action
    )

    $previousExitCode = $global:LASTEXITCODE
    & $Action
    $exitCode = $global:LASTEXITCODE
    if ($null -ne $exitCode -and $exitCode -ne 0) {
        throw "Command failed with exit code $exitCode"
    }
    $global:LASTEXITCODE = $previousExitCode
}

function Measure-Millis([scriptblock]$Action) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-Checked $Action
    $sw.Stop()
    return [int64]$sw.ElapsedMilliseconds
}

function Capture-CheckedOutput([scriptblock]$Action) {
    $previousExitCode = $global:LASTEXITCODE
    $output = & $Action
    $exitCode = $global:LASTEXITCODE
    if ($null -ne $exitCode -and $exitCode -ne 0) {
        throw "Command failed with exit code $exitCode"
    }
    $global:LASTEXITCODE = $previousExitCode
    return $output
}

$coldIndexMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo index --no-semantic | Out-Null }
$warmIndexMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo index --no-semantic | Out-Null }
$semanticIndexReuseMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo refresh-search --semantic-backend local-hash-v1 | Out-Null }
$semanticIndexReembedMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo refresh-search --semantic-backend local-hash-v1 --reembed-all | Out-Null }
$callWarmupMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo warm-calls | Out-Null }
$archMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo get_architecture_overview --format json | Out-Null }
$detectChangesMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo detect_changes main HEAD --format json | Out-Null }
$affectedFlowsMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo get_affected_flows main HEAD --format json | Out-Null }
$callersMs = Measure-Millis { & $ReposcryBin --repo $FixtureRepo query_graph $CallersQuery | Out-Null }
$searchMs = Measure-Millis {
    & $ReposcryBin --repo $FixtureRepo semantic_search_nodes $SemanticQuery --limit 20 --semantic --semantic-backend local-hash-v1 | Out-Null
}
$customSemanticReuseMs = $null
$customSemanticReembedMs = $null
if ($SemanticBenchBackend) {
    $customSemanticReuseMs = Measure-Millis {
        & $ReposcryBin --repo $FixtureRepo refresh-search --semantic-backend $SemanticBenchBackend | Out-Null
    }
    $customSemanticReembedMs = Measure-Millis {
        & $ReposcryBin --repo $FixtureRepo refresh-search --semantic-backend $SemanticBenchBackend --reembed-all | Out-Null
    }
}

$archJson = Capture-CheckedOutput { & $ReposcryBin --repo $FixtureRepo get_architecture_overview --format json }
$arch = $archJson | ConvertFrom-Json
$osLine = ((cmd /c ver) | Where-Object { $_.Trim() -ne "" } | Select-Object -Last 1).Trim()

$dbSize = 0
if (Test-Path (Join-Path $FixtureRepo ".reposcry/reposcry.db")) {
    $dbSize = (Get-Item (Join-Path $FixtureRepo ".reposcry/reposcry.db")).Length
}

$payload = [ordered]@{
    captured_at = [DateTime]::UtcNow.ToString("o")
    machine = [ordered]@{
        os = $osLine
        cpu = $env:PROCESSOR_IDENTIFIER
        memory_gb = if ($env:REPOSCRY_MEMORY_GB) { $env:REPOSCRY_MEMORY_GB } else { "unknown" }
    }
    repo = [ordered]@{
        path = $FixtureRepo.Path
        fixture = $Fixture.name
        size = $Fixture.size
        notes = $Fixture.notes
        fixture_manifest = "benchmarks/fixtures.json"
    }
    metrics = [ordered]@{
        cold_index_ms = $coldIndexMs
        warm_index_ms = $warmIndexMs
        semantic_index_reuse_ms = $semanticIndexReuseMs
        semantic_index_reembed_ms = $semanticIndexReembedMs
        call_warmup_ms = $callWarmupMs
        architecture_overview_ms = $archMs
        detect_changes_ms = $detectChangesMs
        affected_flows_ms = $affectedFlowsMs
        query_graph_callers_ms = $callersMs
        semantic_search_ms = $searchMs
        db_size_bytes = $dbSize
        files_indexed = $arch.files_indexed
        symbols_indexed = $arch.symbols_indexed
        imports_indexed = $arch.imports_indexed
        persisted_call_sites = $arch.persisted_call_sites
        persisted_symbol_call_edges = $arch.persisted_symbol_call_edges
        persisted_file_call_edges = $arch.persisted_file_call_edges
        total_edges = $arch.resolved_import_edges
    }
}

if ($SemanticBenchBackend) {
    $payload.metrics.semantic_index_backend = $SemanticBenchBackend
    $payload.metrics.semantic_index_backend_reuse_ms = $customSemanticReuseMs
    $payload.metrics.semantic_index_backend_reembed_ms = $customSemanticReembedMs
}

$payload | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $OutFile
Get-Content $OutFile
