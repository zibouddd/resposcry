param(
  [string]$FixtureName
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ManifestPath = Join-Path $Root "benchmarks/fixtures.json"
$Fixtures = (Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json).fixtures

foreach ($Fixture in $Fixtures) {
    if ($Fixture.name -eq "current_repo") {
        continue
    }
    if ($FixtureName -and $Fixture.name -ne $FixtureName) {
        continue
    }

    $FixturePath = Resolve-Path (Join-Path $Root $Fixture.path)
    $GitDir = Join-Path $FixturePath ".git"
    if (-not (Test-Path $GitDir)) {
        git -C $FixturePath init | Out-Null
        git -C $FixturePath checkout -B main | Out-Null
        git -C $FixturePath config user.name "RepoScry Fixtures" | Out-Null
        git -C $FixturePath config user.email "fixtures@example.invalid" | Out-Null
        git -C $FixturePath add . | Out-Null
        git -C $FixturePath commit -m "Initial fixture" | Out-Null
        continue
    }

    git -C $FixturePath config user.name "RepoScry Fixtures" | Out-Null
    git -C $FixturePath config user.email "fixtures@example.invalid" | Out-Null
    $headExists = $true
    try {
        git -C $FixturePath rev-parse --verify HEAD | Out-Null
    } catch {
        $headExists = $false
    }
    if (-not $headExists) {
        git -C $FixturePath checkout -B main | Out-Null
        git -C $FixturePath add . | Out-Null
        git -C $FixturePath commit -m "Initial fixture" | Out-Null
    }
}
