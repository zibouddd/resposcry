$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("reposcry-release-smoke-" + [System.Guid]::NewGuid().ToString("N"))
$distDir = Join-Path $tmpRoot "dist"
$installDir = Join-Path $tmpRoot "install-check"
$binaries = "reposcry", "reposcry-update", "reposcry-watch", "reposcry-export", "reposcry-mcp-plus"

try {
    cargo build --release -p reposcry-cli --bins | Out-Null

    foreach ($bin in $binaries) {
        $binary = Join-Path $Root "target/release/$bin.exe"
        if (-not (Test-Path $binary)) {
            throw "Release binary was not produced at $binary"
        }
    }

    New-Item -ItemType Directory -Force -Path $distDir | Out-Null
    foreach ($bin in $binaries) {
        Copy-Item -LiteralPath (Join-Path $Root "target/release/$bin.exe") -Destination (Join-Path $distDir "$bin.exe") -Force
    }

    $asset = "reposcry-x86_64-pc-windows-msvc.zip"
    $assetPath = Join-Path $tmpRoot $asset
    Compress-Archive -Path (Join-Path $distDir "*.exe") -DestinationPath $assetPath -Force

    $hash = (Get-FileHash $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -NoNewline -Path "$assetPath.sha256" -Value "$hash  $asset"

    $env:REPOSCRY_RELEASE_BASE_URL = "file:///$($tmpRoot -replace '\\','/')"
    $env:REPOSCRY_INSTALL_DIR = $installDir
    $env:REPOSCRY_ARCH = "x86_64"
    .\install.ps1 | Out-Null

    foreach ($bin in $binaries) {
        $installed = Join-Path $installDir "$bin.exe"
        if (-not (Test-Path $installed)) {
            throw "Installed $bin.exe was not found"
        }
        $version = & $installed --version
        if ($version -notmatch ('^' + [regex]::Escape($bin) + ' ')) {
            throw "Unexpected version output for $bin.exe`: $version"
        }
    }

    Write-Output "Release smoke passed"
} finally {
    Remove-Item Env:REPOSCRY_RELEASE_BASE_URL -ErrorAction SilentlyContinue
    Remove-Item Env:REPOSCRY_INSTALL_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:REPOSCRY_ARCH -ErrorAction SilentlyContinue
    if (Test-Path $tmpRoot) {
        Remove-Item -Recurse -Force $tmpRoot
    }
}
