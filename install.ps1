$ErrorActionPreference = "Stop"

$repo = if ($env:REPOSCRY_REPO) { $env:REPOSCRY_REPO } else { "zibouddd/reposcry" }
$version = if ($env:REPOSCRY_VERSION) { $env:REPOSCRY_VERSION } else { "latest" }
$installDir = if ($env:REPOSCRY_INSTALL_DIR) {
    $env:REPOSCRY_INSTALL_DIR
} elseif ($env:LOCALAPPDATA) {
    Join-Path $env:LOCALAPPDATA "Programs\reposcry\bin"
} else {
    Join-Path $HOME ".local\bin"
}

$archInput = if ($env:REPOSCRY_ARCH) { $env:REPOSCRY_ARCH } else { $env:PROCESSOR_ARCHITECTURE }
switch -Regex ($archInput) {
    "^(AMD64|x86_64)$" { $arch = "x86_64" }
    "^(ARM64|aarch64)$" { $arch = "aarch64" }
    default { throw "Unsupported architecture: $archInput" }
}

$target = "$arch-pc-windows-msvc"
$asset = "reposcry-$target.zip"
$checksumAsset = "$asset.sha256"

if ($env:REPOSCRY_RELEASE_BASE_URL) {
    $baseUrl = $env:REPOSCRY_RELEASE_BASE_URL.TrimEnd("/")
} elseif ($version -eq "latest") {
    $baseUrl = "https://github.com/$repo/releases/latest/download"
} else {
    $baseUrl = "https://github.com/$repo/releases/download/$version"
}

$assetUrl = "$baseUrl/$asset"
$checksumUrl = "$baseUrl/$checksumAsset"
$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("reposcry-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

function Remove-TempDir {
    if (Test-Path $tmpDir) {
        Remove-Item -Recurse -Force $tmpDir
    }
}

function Resolve-FileUriPath {
    param([string]$UriString)
    $uri = [System.Uri]$UriString
    $path = $uri.LocalPath
    if ($path -match '^/[A-Za-z]:/') {
        return $path.Substring(1)
    }
    return $path
}

function Fetch-Asset {
    param(
        [string]$Url,
        [string]$Destination
    )

    if ($Url.StartsWith("file://", [System.StringComparison]::OrdinalIgnoreCase)) {
        Copy-Item -LiteralPath (Resolve-FileUriPath $Url) -Destination $Destination -Force
    } else {
        Invoke-WebRequest -Uri $Url -OutFile $Destination
    }
}

try {
    $assetPath = Join-Path $tmpDir $asset
    $checksumPath = Join-Path $tmpDir $checksumAsset
    Fetch-Asset -Url $assetUrl -Destination $assetPath
    Fetch-Asset -Url $checksumUrl -Destination $checksumPath

    $expectedLine = (Get-Content -LiteralPath $checksumPath | Select-Object -First 1).Trim()
    if (-not $expectedLine) {
        throw "Checksum file is empty: $checksumAsset"
    }
    $expectedHash = ($expectedLine -split '\s+')[0].ToLowerInvariant()
    $actualHash = (Get-FileHash -LiteralPath $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($expectedHash -ne $actualHash) {
        throw "Checksum mismatch for $asset"
    }

    $extractDir = Join-Path $tmpDir "extract"
    Expand-Archive -LiteralPath $assetPath -DestinationPath $extractDir -Force

    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    foreach ($binaryName in @("reposcry.exe", "reposcry-update.exe", "reposcry-watch.exe", "reposcry-export.exe", "reposcry-mcp-plus.exe")) {
        $binaryPath = Join-Path $extractDir $binaryName
        if (-not (Test-Path $binaryPath)) {
            throw "Archive did not contain $binaryName"
        }
        $destination = Join-Path $installDir $binaryName
        Copy-Item -LiteralPath $binaryPath -Destination $destination -Force
        Write-Output "Installed $binaryName to $destination"
    }
} finally {
    Remove-TempDir
}
