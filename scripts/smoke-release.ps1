$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("reposcry-release-smoke-" + [System.Guid]::NewGuid().ToString("N"))
$distDir = Join-Path $tmpRoot "dist"
$installDir = Join-Path $tmpRoot "install-check"

try {
    cargo build --release -p reposcry-cli --bins | Out-Null

    foreach ($binaryName in @("reposcry.exe", "reposcry-update.exe")) {
        $binary = Join-Path $Root "target/release/$binaryName"
        if (-not (Test-Path $binary)) {
            throw "Release binary was not produced at $binary"
        }
    }

    New-Item -ItemType Directory -Force -Path $distDir | Out-Null
    Copy-Item -LiteralPath (Join-Path $Root "target/release/reposcry.exe") -Destination (Join-Path $distDir "reposcry.exe") -Force
    Copy-Item -LiteralPath (Join-Path $Root "target/release/reposcry-update.exe") -Destination (Join-Path $distDir "reposcry-update.exe") -Force

    $asset = "reposcry-x86_64-pc-windows-msvc.zip"
    $assetPath = Join-Path $tmpRoot $asset
    Compress-Archive -Path (Join-Path $distDir "reposcry.exe"), (Join-Path $distDir "reposcry-update.exe") -DestinationPath $assetPath -Force

    $hash = (Get-FileHash $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -NoNewline -Path "$assetPath.sha256" -Value "$hash  $asset"

    $env:REPOSCRY_RELEASE_BASE_URL = "file:///$($tmpRoot -replace '\\','/')"
    $env:REPOSCRY_INSTALL_DIR = $installDir
    $env:REPOSCRY_ARCH = "x86_64"
    .\install.ps1 | Out-Null

    foreach ($binaryName in @("reposcry.exe", "reposcry-update.exe")) {
        $installed = Join-Path $installDir $binaryName
        if (-not (Test-Path $installed)) {
            throw "Installed $binaryName was not found"
        }
        $version = & $installed --version
        if ($version -notmatch ('^' + [regex]::Escape($binaryName.Replace('.exe', '')) + ' ')) {
            throw "Unexpected version output for $binaryName`: $version"
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
