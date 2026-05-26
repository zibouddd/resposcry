$packages = @(
    "reposcry-graph",
    "reposcry-cache",
    "reposcry-git",
    "reposcry-indexer",
    "reposcry-rules",
    "reposcry-context",
    "reposcry-report",
    "reposcry-cli"
)

foreach ($pkg in $packages) {
    Write-Host "Publishing $pkg..." -ForegroundColor Cyan
    cargo publish -p $pkg
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Failed to publish $pkg. Aborting." -ForegroundColor Red
        exit 1
    }
    Write-Host "Published $pkg successfully." -ForegroundColor Green
    # Small delay to let crates.io index the package before the next one depends on it
    Start-Sleep -Seconds 20
}

Write-Host "All packages published successfully!" -ForegroundColor Green