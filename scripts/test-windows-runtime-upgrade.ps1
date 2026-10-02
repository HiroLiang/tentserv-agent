# Offline invocation test for the real installer bootstrap function.
$ErrorActionPreference = "Stop"
$installer = Join-Path $PSScriptRoot "install.ps1"
if ((Get-Content -Raw -LiteralPath $installer) -notmatch 'else \{ "3\.12" \}') {
    throw "Installer must default to managed Python 3.12"
}
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($installer, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw "Installer parse failed: $parseErrors" }
$bootstrap = $ast.Find({
    param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq "Bootstrap-PythonEnv"
}, $true)
if (-not $bootstrap) { throw "Installer bootstrap function not found" }
. ([scriptblock]::Create($bootstrap.Extent.Text))
function Fail($Message) { throw $Message }

$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("tentgent-windows-upgrade-" + [guid]::NewGuid())
$savedEnvironment = @{}
$environmentNames = @(
    "TENTGENT_BOOTSTRAP_UV", "TENTGENT_PYTHON_DIR", "TENTGENT_PYTHON_ENV_DIR",
    "TENTGENT_BOOTSTRAP_UV_CACHE_DIR", "UV_PROJECT_ENVIRONMENT", "UV_MANAGED_PYTHON",
    "UV_CACHE_DIR", "TENTGENT_TEST_UV_ARGS"
)
foreach ($name in $environmentNames) {
    $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name)
    [Environment]::SetEnvironmentVariable($name, $null)
}
try {
    $shareDir = Join-Path $testRoot "share"
    $projectDir = Join-Path $shareDir "python/tentgent-model-runtime"
    New-Item -ItemType Directory -Force -Path (Join-Path $projectDir "src") | Out-Null
    Set-Content -LiteralPath (Join-Path $projectDir "pyproject.toml") -Value '[project]' -Encoding UTF8
    $fakeUv = Join-Path $testRoot "fake-uv.ps1"
    @'
$ErrorActionPreference = "Stop"
$args | ConvertTo-Json | Set-Content -LiteralPath $env:TENTGENT_TEST_UV_ARGS
$reinstallIndex = [Array]::IndexOf($args, "--reinstall-package")
if ($reinstallIndex -lt 0 -or $args[$reinstallIndex + 1] -ne "tentgent-model-runtime") {
    throw "runtime source refresh was not requested"
}
$scriptsDir = Join-Path $env:UV_PROJECT_ENVIRONMENT "Scripts"
New-Item -ItemType Directory -Force -Path $scriptsDir | Out-Null
foreach ($name in @("python.exe", "tentgent-model-runtime-daemon.exe", "tentgent-hf-snapshot.exe")) {
    Set-Content -LiteralPath (Join-Path $scriptsDir $name) -Value "fixture"
}
$global:LASTEXITCODE = 0
'@ | Set-Content -LiteralPath $fakeUv -Encoding UTF8
    $env:TENTGENT_BOOTSTRAP_UV = $fakeUv
    $env:TENTGENT_TEST_UV_ARGS = Join-Path $testRoot "uv-args.json"
    $PythonVersion = "3.12"
    Bootstrap-PythonEnv $testRoot $shareDir "x86_64-pc-windows-msvc"
    $arguments = Get-Content -Raw -LiteralPath $env:TENTGENT_TEST_UV_ARGS | ConvertFrom-Json
    foreach ($required in @("sync", "--frozen", "--no-editable", "--reinstall-package", "tentgent-model-runtime")) {
        if ($required -notin $arguments) { throw "Missing bootstrap argument: $required" }
    }
    $pythonIndex = [Array]::IndexOf($arguments, "--python")
    if ($pythonIndex -lt 0 -or $arguments[$pythonIndex + 1] -ne "3.12") {
        throw "Installer did not select Python 3.12"
    }
    Write-Host "Windows runtime reinstall invocation test passed"
} finally {
    foreach ($name in $environmentNames) {
        [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name])
    }
    Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
}
