param(
    [string]$Archive = "",
    [string]$Checksums = "",
    [string]$Version = "0.3.1",
    [string]$Prefix = "",
    [string]$Target = "",
    [switch]$DryRun,
    [switch]$SkipPythonBootstrap,
    [switch]$SkipDoctor
)

$ErrorActionPreference = "Stop"

$DefaultBaseUrl = "https://agent.tentserv.com/releases"
$UvVersion = "0.11.7"
$UvSha256SumSha256 = "2d56c5c54e3027c2c26e4f0cc1383be99a7af0a6b39dc7f2a5c6f2e5aa8878e4"
$PythonVersion = if ($env:TENTGENT_BOOTSTRAP_PYTHON_VERSION) { $env:TENTGENT_BOOTSTRAP_PYTHON_VERSION } else { "3.13" }

function Fail($Message) {
    Write-Error "error: $Message"
    exit 1
}

function Resolve-Target {
    if ($Target) {
        return $Target
    }
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -eq "AMD64") {
        return "x86_64-pc-windows-msvc"
    }
    Fail "unsupported Windows architecture: $arch; pass -Target to override"
}

function Resolve-Prefix {
    if ($Prefix) {
        return $Prefix
    }
    if ($env:TENTGENT_INSTALL_PREFIX) {
        return $env:TENTGENT_INSTALL_PREFIX
    }
    if (-not $env:LOCALAPPDATA) {
        Fail "LOCALAPPDATA is required; pass -Prefix to override"
    }
    return Join-Path $env:LOCALAPPDATA "Programs\tentgent"
}

function Resolve-RuntimeHome {
    if ($env:TENTGENT_HOME) {
        return $env:TENTGENT_HOME
    }
    if (-not $env:LOCALAPPDATA) {
        Fail "LOCALAPPDATA is required; set TENTGENT_HOME to override"
    }
    return Join-Path $env:LOCALAPPDATA "tentserv\tentgent\data"
}

function Download-Or-Copy($Source, $Destination) {
    if ($Source.StartsWith("https://")) {
        Invoke-WebRequest -Uri $Source -OutFile $Destination
        return
    }
    if ($Source.StartsWith("file://")) {
        $uri = [Uri]$Source
        Copy-Item -LiteralPath $uri.LocalPath -Destination $Destination -Force
        return
    }
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

function Get-Sha256($Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Verify-Sha256($Path, $Expected) {
    $actual = Get-Sha256 $Path
    if ($actual -ne $Expected.ToLowerInvariant()) {
        Fail "checksum mismatch for ${Path}: expected ${Expected}, got ${actual}"
    }
}

function Get-ChecksumForArchive($ChecksumsPath, $ArchiveName) {
    foreach ($line in Get-Content -LiteralPath $ChecksumsPath) {
        $trimmed = $line.Trim()
        if (-not $trimmed) {
            continue
        }
        $parts = $trimmed -split "\s+"
        if ($parts.Count -lt 2) {
            continue
        }
        $name = $parts[1].TrimStart("*")
        if ($name -eq $ArchiveName) {
            return $parts[0]
        }
    }
    Fail "checksum entry not found for $ArchiveName"
}

function Get-UvAsset($BootstrapTarget) {
    switch ($BootstrapTarget) {
        "x86_64-pc-windows-msvc" { return "uv-x86_64-pc-windows-msvc.zip" }
        "aarch64-pc-windows-msvc" { return "uv-aarch64-pc-windows-msvc.zip" }
        default { Fail "unsupported uv bootstrap target: $BootstrapTarget" }
    }
}

function Get-ExpectedAssetSha($SumsPath, $Asset) {
    foreach ($line in Get-Content -LiteralPath $SumsPath) {
        $parts = $line.Trim() -split "\s+"
        if ($parts.Count -lt 2) {
            continue
        }
        $name = $parts[1].TrimStart("*")
        if ($name -eq $Asset) {
            return $parts[0]
        }
    }
    Fail "checksum entry not found for $Asset"
}

function Ensure-PinnedUv($RuntimeHome, $BootstrapTarget) {
    $asset = Get-UvAsset $BootstrapTarget
    $cacheDir = if ($env:TENTGENT_BOOTSTRAP_CACHE_DIR) {
        $env:TENTGENT_BOOTSTRAP_CACHE_DIR
    } else {
        Join-Path $RuntimeHome "runtime\bootstrap"
    }
    $toolDir = Join-Path $cacheDir "uv\$UvVersion\$BootstrapTarget"
    $uvPath = Join-Path $toolDir "bin\uv.exe"
    $manifestPath = Join-Path $toolDir "manifest.toml"

    if ((Test-Path -LiteralPath $uvPath) -and -not $env:TENTGENT_BOOTSTRAP_FORCE) {
        Write-Host "==> Pinned uv already cached"
        return $uvPath
    }

    $uvBaseUrl = "https://github.com/astral-sh/uv/releases/download/$UvVersion"
    $sumsUrl = "$uvBaseUrl/sha256.sum"
    $archiveUrl = "$uvBaseUrl/$asset"
    $tmpDir = Join-Path $toolDir ".tmp.$PID"
    $extractDir = Join-Path $tmpDir "extract"
    $sumsPath = Join-Path $tmpDir "sha256.sum"
    $archivePath = Join-Path $tmpDir $asset

    Remove-Item -LiteralPath $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $extractDir, (Join-Path $toolDir "bin") | Out-Null

    try {
        Write-Host "==> Downloading uv checksum manifest $UvVersion"
        Invoke-WebRequest -Uri $sumsUrl -OutFile $sumsPath
        Verify-Sha256 $sumsPath $UvSha256SumSha256

        $expectedAssetSha = Get-ExpectedAssetSha $sumsPath $asset
        Write-Host "==> Downloading pinned uv $UvVersion for $BootstrapTarget"
        Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath
        Verify-Sha256 $archivePath $expectedAssetSha

        Write-Host "==> Extracting uv"
        Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDir -Force
        $extractedUv = Get-ChildItem -Path $extractDir -Recurse -Filter "uv.exe" | Select-Object -First 1
        if (-not $extractedUv) {
            Fail "uv.exe was not found in $asset"
        }

        Copy-Item -LiteralPath $extractedUv.FullName -Destination $uvPath -Force
        Copy-Item -LiteralPath $sumsPath -Destination (Join-Path $toolDir "sha256.sum") -Force

        @"
tool = "uv"
version = "$UvVersion"
target = "$BootstrapTarget"
asset = "$asset"
url = "$archiveUrl"
sha256 = "$expectedAssetSha"
checksum_manifest_url = "$sumsUrl"
checksum_manifest_sha256 = "$UvSha256SumSha256"
uv_path = "$uvPath"
"@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8
    } finally {
        Remove-Item -LiteralPath $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    return $uvPath
}

function Bootstrap-PythonEnv($RuntimeHome, $ShareDir, $BootstrapTarget) {
    $projectDir = if ($env:TENTGENT_PYTHON_DIR) {
        $env:TENTGENT_PYTHON_DIR
    } else {
        Join-Path $ShareDir "python"
    }
    $envDir = if ($env:TENTGENT_PYTHON_ENV_DIR) {
        $env:TENTGENT_PYTHON_ENV_DIR
    } else {
        Join-Path $RuntimeHome "runtime\python-env"
    }
    $uvCacheDir = if ($env:TENTGENT_BOOTSTRAP_UV_CACHE_DIR) {
        $env:TENTGENT_BOOTSTRAP_UV_CACHE_DIR
    } else {
        Join-Path $RuntimeHome "runtime\bootstrap\uv-cache"
    }

    if (-not (Test-Path -LiteralPath (Join-Path $projectDir "pyproject.toml"))) {
        Fail "Python daemon pyproject.toml is missing: $projectDir"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $projectDir "src"))) {
        Fail "Python daemon src directory is missing: $projectDir\src"
    }

    $uvPath = if ($env:TENTGENT_BOOTSTRAP_UV) {
        $env:TENTGENT_BOOTSTRAP_UV
    } else {
        Ensure-PinnedUv $RuntimeHome $BootstrapTarget
    }
    if (-not (Test-Path -LiteralPath $uvPath)) {
        Fail "pinned uv is missing or not executable: $uvPath"
    }

    Write-Host "python project: $projectDir"
    Write-Host "python env: $envDir"
    Write-Host "python version: $PythonVersion"
    Write-Host "uv cache: $uvCacheDir"
    Write-Host "uv path: $uvPath"
    Write-Host "==> Syncing managed Python environment"

    New-Item -ItemType Directory -Force -Path $uvCacheDir | Out-Null
    $env:UV_PROJECT_ENVIRONMENT = $envDir
    $env:UV_MANAGED_PYTHON = "1"
    $env:UV_CACHE_DIR = $uvCacheDir
    & $uvPath --no-config sync --project $projectDir --managed-python --python $PythonVersion --frozen --no-editable
    if ($LASTEXITCODE -ne 0) {
        Fail "uv sync failed with exit code $LASTEXITCODE"
    }

    $scriptsDir = Join-Path $envDir "Scripts"
    $required = @(
        "python.exe",
        "tentgent-chat-once.exe",
        "tentgent-server.exe",
        "tentgent-train-lora-run.exe",
        "tentgent-hf-snapshot.exe"
    )
    foreach ($name in $required) {
        $path = Join-Path $scriptsDir $name
        if (-not (Test-Path -LiteralPath $path)) {
            Fail "missing expected Python runtime entry point: $path"
        }
    }

    Write-Host "==> Python runtime environment ready"
    Write-Host $envDir
}

$Target = Resolve-Target
$Prefix = Resolve-Prefix
$RuntimeHome = Resolve-RuntimeHome
$PackageName = "tentgent-$Version-$Target"
$ArchiveName = "$PackageName.zip"
$BaseUrl = if ($env:TENTGENT_INSTALL_BASE_URL) { $env:TENTGENT_INSTALL_BASE_URL } else { "$DefaultBaseUrl/$Version" }
$ArchiveSource = if ($Archive) { $Archive } else { "$BaseUrl/$ArchiveName" }
$ChecksumsSource = if ($Checksums) { $Checksums } else { "$BaseUrl/checksums.txt" }
$BinDir = Join-Path $Prefix "bin"
$ShareDir = Join-Path $Prefix "share\tentgent"

Write-Host "version: $Version"
Write-Host "target: $Target"
Write-Host "archive: $ArchiveSource"
Write-Host "checksums: $ChecksumsSource"
Write-Host "prefix: $Prefix"
Write-Host "bin dir: $BinDir"
Write-Host "share dir: $ShareDir"
Write-Host "runtime home: $RuntimeHome"
Write-Host "python bootstrap: $(-not $SkipPythonBootstrap)"
Write-Host "doctor: $(-not $SkipDoctor)"

if ($DryRun) {
    exit 0
}

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) "tentgent-install-$PID"
$archivePath = Join-Path $tmpDir $ArchiveName
$checksumsPath = Join-Path $tmpDir "checksums.txt"
$extractDir = Join-Path $tmpDir "extract"

Remove-Item -LiteralPath $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $extractDir | Out-Null

try {
    Write-Host "==> Fetching Tentgent archive"
    Download-Or-Copy $ArchiveSource $archivePath

    Write-Host "==> Fetching checksums"
    Download-Or-Copy $ChecksumsSource $checksumsPath

    $expectedSha = Get-ChecksumForArchive $checksumsPath $ArchiveName
    Verify-Sha256 $archivePath $expectedSha

    Write-Host "==> Installing Tentgent to $Prefix"
    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDir -Force

    $binarySource = Join-Path $extractDir "bin\tentgent.exe"
    $pythonSource = Join-Path $extractDir "share\tentgent\python"
    $scriptsSource = Join-Path $extractDir "share\tentgent\scripts"
    if (-not (Test-Path -LiteralPath $binarySource)) { Fail "archive is missing bin\tentgent.exe" }
    if (-not (Test-Path -LiteralPath $pythonSource)) { Fail "archive is missing share\tentgent\python" }
    if (-not (Test-Path -LiteralPath $scriptsSource)) { Fail "archive is missing share\tentgent\scripts" }

    New-Item -ItemType Directory -Force -Path $BinDir, $ShareDir | Out-Null
    Copy-Item -LiteralPath $binarySource -Destination (Join-Path $BinDir "tentgent.exe") -Force
    Remove-Item -LiteralPath (Join-Path $ShareDir "python"), (Join-Path $ShareDir "scripts") -Recurse -Force -ErrorAction SilentlyContinue
    Copy-Item -LiteralPath $pythonSource -Destination (Join-Path $ShareDir "python") -Recurse -Force
    Copy-Item -LiteralPath $scriptsSource -Destination (Join-Path $ShareDir "scripts") -Recurse -Force

    if (-not $SkipPythonBootstrap) {
        Write-Host "==> Bootstrapping managed Python runtime"
        Bootstrap-PythonEnv $RuntimeHome $ShareDir $Target
    } else {
        Write-Host "==> Skipping Python bootstrap"
    }

    if (-not $SkipDoctor) {
        Write-Host "==> Running tentgent doctor"
        & (Join-Path $BinDir "tentgent.exe") doctor
        if ($LASTEXITCODE -ne 0) {
            Fail "tentgent doctor failed with exit code $LASTEXITCODE"
        }
    } else {
        Write-Host "==> Skipping tentgent doctor"
    }
} finally {
    Remove-Item -LiteralPath $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "==> Tentgent installed"
Write-Host "binary: $(Join-Path $BinDir 'tentgent.exe')"
Write-Host ""
Write-Host "If $BinDir is not on PATH, add it before running tentgent."
Write-Host "Verify later with:"
Write-Host "  tentgent doctor"
