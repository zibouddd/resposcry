$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ManifestPath = Join-Path $Root "benchmarks/fixtures.json"
$Fixtures = (Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json).fixtures

foreach ($Fixture in $Fixtures) {
    if ($Fixture.name -eq "current_repo") {
        Remove-Item Env:REPOSCRY_BENCH_FIXTURE -ErrorAction SilentlyContinue
    } else {
        $env:REPOSCRY_BENCH_FIXTURE = $Fixture.name
    }
    Remove-Item Env:BENCH_OUT_NAME -ErrorAction SilentlyContinue
    & (Join-Path $PSScriptRoot "bench.ps1")
}
