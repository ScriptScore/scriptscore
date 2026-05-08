# SPDX-License-Identifier: AGPL-3.0-only

param(
    [switch] $CheckPrerequisitesOnly,
    [switch] $SkipRustCoverage,
    [switch] $IncludeUnsafeReport,
    [int] $UnsafeReportTimeoutSeconds = 3600
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$DesktopDir = Resolve-Path (Join-Path $ScriptDir "..")
$RepoRoot = Resolve-Path (Join-Path $DesktopDir "..")
$WorkspaceRoot = Split-Path $RepoRoot -Parent
$WorkspaceToolsRoot = Join-Path $WorkspaceRoot ".tools"
$RepoToolsRoot = Join-Path $RepoRoot ".tools"
$CargoToolsRoot = Join-Path $RepoToolsRoot "cargo-tools"
$CargoToolsBin = Join-Path $CargoToolsRoot "bin"
$ScratchRoot = Join-Path $RepoRoot "scratch-review-quality"
$ArtifactDir = Join-Path $RepoRoot "artifacts"

function Add-PathFront {
    param([string] $PathEntry)

    if (Test-Path $PathEntry) {
        $entries = $env:PATH -split ";" | Where-Object { $_ -and $_ -ne $PathEntry }
        $env:PATH = (@($PathEntry) + $entries) -join ";"
    }
}

function Initialize-ToolPath {
    $cargoRoot = Join-Path $WorkspaceToolsRoot "cargo"
    $rustupRoot = Join-Path $WorkspaceToolsRoot "rustup"
    $nodeRoot = Join-Path $WorkspaceToolsRoot "node-v20.20.2-win-x64"
    $uvRoot = Join-Path $WorkspaceToolsRoot "uv"
    $gitBin = Join-Path $WorkspaceToolsRoot "mingit\cmd"

    if (Test-Path $cargoRoot) {
        $env:CARGO_HOME = $cargoRoot
    }
    if (Test-Path $rustupRoot) {
        $env:RUSTUP_HOME = $rustupRoot
    }

    Add-PathFront (Join-Path $cargoRoot "bin")
    Add-PathFront $CargoToolsBin
    Add-PathFront $uvRoot
    Add-PathFront $nodeRoot
    Add-PathFront $gitBin
}

function Import-VsDevEnvironment {
    if (Get-Command cl.exe -ErrorAction SilentlyContinue) {
        return
    }

    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (!(Test-Path $vswhere)) {
        return
    }

    $installPath = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (!$installPath) {
        return
    }

    $vsDevCmd = Join-Path $installPath "Common7\Tools\VsDevCmd.bat"
    if (!(Test-Path $vsDevCmd)) {
        return
    }

    $envLines = cmd.exe /c "`"$vsDevCmd`" -arch=x64 >nul && set"
    foreach ($line in $envLines) {
        $name, $value = $line -split "=", 2
        if ($name -and $value) {
            Set-Item -Path "Env:$name" -Value $value
        }
    }

    $env:PATH = (($env:PATH -split ";") | Where-Object {
        $_ -and $_ -notlike "*Microsoft Visual Studio*Common7*IDE*"
    }) -join ";"
}

function Resolve-RequiredCommand {
    param(
        [string] $Name,
        [string[]] $CandidatePaths
    )

    foreach ($candidate in $CandidatePaths) {
        if ($candidate -and (Test-Path $candidate)) {
            return $candidate
        }
    }

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    throw "$Name was not found. Run scripts/windows-dev.ps1 or install the missing tool, then retry."
}

function Invoke-Step {
    param(
        [string] $Name,
        [scriptblock] $Script
    )

    Write-Host ""
    Write-Host "==> $Name"
    $elapsed = [System.Diagnostics.Stopwatch]::StartNew()
    & $Script
    $elapsed.Stop()
    Write-Host ("<== {0} ({1:n1}s)" -f $Name, $elapsed.Elapsed.TotalSeconds)
}

function Install-CargoTool {
    param(
        [string] $ToolName,
        [string] $Version,
        [string[]] $InstallArgs = @()
    )

    $exeName = "$ToolName.exe"
    $toolPath = Join-Path $CargoToolsBin $exeName
    $stampDir = Join-Path $RepoToolsRoot "tool-stamps"
    $stampPath = Join-Path $stampDir "$ToolName-$Version.stamp"

    New-Item -ItemType Directory -Force -Path $CargoToolsBin, $stampDir | Out-Null
    if ((Test-Path $toolPath) -and (Test-Path $stampPath)) {
        return $toolPath
    }

    Get-ChildItem -Path $stampDir -Filter "$ToolName-*.stamp" -ErrorAction SilentlyContinue |
        Remove-Item -Force

    $installCommandArgs = @(
        "install",
        "--locked",
        "--force",
        "--root",
        $CargoToolsRoot,
        $ToolName,
        "--version",
        $Version
    ) + $InstallArgs
    & cargo @installCommandArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo install failed for $ToolName"
    }
    New-Item -ItemType File -Force -Path $stampPath | Out-Null
    $toolPath
}

function Invoke-RustCodeAnalysis {
    $tool = Install-CargoTool -ToolName "rust-code-analysis-cli" -Version "0.0.25"
    $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("scriptscore-rust-analysis-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

    try {
        $srcRaw = Join-Path $tmpDir "src.json"
        $testsRaw = Join-Path $tmpDir "tests.json"
        $version = (& $tool --version).Split(" ")[1]

        & cmd.exe /c "`"$tool`" -m -p `"$RepoRoot\desktop\src-tauri\src`" -I *.rs -O json > `"$srcRaw`""
        if ($LASTEXITCODE -ne 0) {
            throw "rust-code-analysis failed for desktop/src-tauri/src"
        }

        & cmd.exe /c "`"$tool`" -m -p `"$RepoRoot\desktop\src-tauri\tests`" -I *.rs -O json > `"$testsRaw`""
        if ($LASTEXITCODE -ne 0) {
            throw "rust-code-analysis failed for desktop/src-tauri/tests"
        }

        & $PythonExe (Join-Path $RepoRoot "desktop\scripts\rust_code_analysis_report.py") `
            --src-input $srcRaw `
            --tests-input $testsRaw `
            --report-output (Join-Path $ArtifactDir "rust-code-analysis.json") `
            --summary-output (Join-Path $ArtifactDir "rust-code-analysis-summary.json") `
            --tool-version $version
        if ($LASTEXITCODE -ne 0) {
            throw "rust_code_analysis_report.py failed"
        }
    }
    finally {
        if (Test-Path $tmpDir) {
            Remove-Item -Recurse -Force -LiteralPath $tmpDir
        }
    }
}

function Invoke-RustCoverage {
    Install-CargoTool -ToolName "cargo-tarpaulin" -Version "0.34.1" | Out-Null
    $coverageDir = Join-Path $ArtifactDir "coverage"
    New-Item -ItemType Directory -Force -Path $coverageDir | Out-Null
    $env:SCRIPTSCORE_PYTHON = $PythonExe
    & cargo tarpaulin `
        --manifest-path "desktop/src-tauri/Cargo.toml" `
        --workspace `
        --all-features `
        --all-targets `
        --engine llvm `
        --out Lcov `
        --out Xml `
        --output-dir $coverageDir
    if ($LASTEXITCODE -ne 0) {
        throw "cargo tarpaulin failed"
    }
}

function Invoke-UnsafeReport {
    Install-CargoTool -ToolName "cargo-geiger" -Version "0.13.0" -InstallArgs @("--features", "vendored-openssl") | Out-Null

    $reportPath = Join-Path $ArtifactDir "cargo-geiger.json"
    $logPath = Join-Path $ArtifactDir "cargo-geiger.log"
    if (Test-Path $reportPath) {
        Remove-Item -Force $reportPath
    }
    if (Test-Path $logPath) {
        Remove-Item -Force $logPath
    }

    $command = "cd /d `"$RepoRoot\desktop\src-tauri`" && cargo geiger -q --output-format Json --all-features --all-targets > `"$reportPath`" 2> `"$logPath`""
    $process = Start-Process -FilePath "cmd.exe" -ArgumentList @("/c", $command) -PassThru -NoNewWindow

    Wait-Process -Id $process.Id -Timeout $UnsafeReportTimeoutSeconds -ErrorAction SilentlyContinue
    $process.Refresh()
    if (!$process.HasExited) {
        taskkill.exe /PID $process.Id /T /F | Out-Null
        throw "cargo-geiger timed out after ${UnsafeReportTimeoutSeconds}s. Diagnostics: $logPath"
    }

    if ($process.ExitCode -ne 0) {
        $hasReport = (Test-Path $reportPath) -and ((Get-Item $reportPath).Length -gt 0)
        if ($hasReport) {
            Write-Host "cargo-geiger exited non-zero but produced a report; preserving the report."
            return
        }
        if ((Test-Path $logPath) -and (Select-String -Path $logPath -Pattern "error: Found " -Quiet)) {
            Select-String -Path $logPath -Pattern "error: Found " | Select-Object -Last 1 | ForEach-Object { $_.Line }
            Write-Host "cargo-geiger reported unsafe usage; preserving diagnostics."
            return
        }
        throw "cargo-geiger exited before producing a report. Diagnostics: $logPath"
    }
}

Initialize-ToolPath
Import-VsDevEnvironment

$UvExe = Resolve-RequiredCommand -Name "uv" -CandidatePaths @((Join-Path $WorkspaceToolsRoot "uv\uv.exe"))
$NpmExe = Resolve-RequiredCommand -Name "npm.cmd" -CandidatePaths @((Join-Path $WorkspaceToolsRoot "node-v20.20.2-win-x64\npm.cmd"))
$PythonExe = Resolve-RequiredCommand -Name "python" -CandidatePaths @((Join-Path $RepoRoot "cli\.venv\Scripts\python.exe"))
Resolve-RequiredCommand -Name "cargo" -CandidatePaths @() | Out-Null

New-Item -ItemType Directory -Force -Path $ScratchRoot, $ArtifactDir | Out-Null

if ($CheckPrerequisitesOnly) {
    Write-Host "Windows review-quality prerequisites are available."
    Write-Host "uv: $UvExe"
    Write-Host "npm: $NpmExe"
    Write-Host "python: $PythonExe"
    exit 0
}

Push-Location $RepoRoot
try {
    Invoke-Step "cargo fmt" {
        & cargo fmt --check --manifest-path "desktop/src-tauri/Cargo.toml"
        if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed" }
    }
    Invoke-Step "rust clippy" {
        & cargo clippy --manifest-path "desktop/src-tauri/Cargo.toml" --workspace --all-targets --all-features -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }
    }
    Invoke-Step "frontend lint" {
        & $NpmExe --prefix "desktop/frontend" run lint
        if ($LASTEXITCODE -ne 0) { throw "frontend lint failed" }
    }
    Invoke-Step "CLI lint" {
        $env:UV_CACHE_DIR = Join-Path $ScratchRoot "uv-cache"
        & $UvExe --directory "cli" run ruff check . --cache-dir (Join-Path $ScratchRoot "ruff-cache")
        if ($LASTEXITCODE -ne 0) { throw "ruff check failed" }
        & $UvExe --directory "cli" run ruff format --check . --cache-dir (Join-Path $ScratchRoot "ruff-cache")
        if ($LASTEXITCODE -ne 0) { throw "ruff format failed" }
    }
    Invoke-Step "CLI quality" {
        $env:UV_CACHE_DIR = Join-Path $ScratchRoot "uv-cache"
        $env:MYPY_CACHE_DIR = Join-Path $ScratchRoot "mypy-cache"
        $env:COVERAGE_FILE = Join-Path $ScratchRoot "scriptscore-cli.coverage"
        $pytestCache = Join-Path $ScratchRoot "pytest-cache"
        $env:PYTEST_ADDOPTS = "-o cache_dir=$pytestCache $env:PYTEST_ADDOPTS"
        & $UvExe --directory "cli" run mypy
        if ($LASTEXITCODE -ne 0) { throw "mypy failed" }
        & $UvExe --directory "cli" run pytest -q --cov
        if ($LASTEXITCODE -ne 0) { throw "CLI pytest coverage failed" }
    }
    Invoke-Step "frontend tests" {
        & $NpmExe --prefix "desktop/frontend" test
        if ($LASTEXITCODE -ne 0) { throw "frontend tests failed" }
    }
    Invoke-Step "frontend coverage" {
        & $NpmExe --prefix "desktop/frontend" run coverage
        if ($LASTEXITCODE -ne 0) { throw "frontend coverage failed" }
    }
    if (!$SkipRustCoverage) {
        Invoke-Step "Rust coverage" {
            Invoke-RustCoverage
        }
    }
    else {
        Write-Host "Skipping Rust coverage because -SkipRustCoverage was provided."
    }
    Invoke-Step "quality metrics" {
        & $PythonExe -m unittest discover -s "desktop/scripts/tests" -p "test_*.py"
        if ($LASTEXITCODE -ne 0) { throw "desktop script tests failed" }
        Invoke-RustCodeAnalysis
    }
    if ($IncludeUnsafeReport) {
        Invoke-Step "unsafe report" {
            Invoke-UnsafeReport
        }
    }
    else {
        Write-Host "Skipping cargo-geiger unsafe report. Pass -IncludeUnsafeReport to attempt it on Windows."
    }
    Invoke-Step "license compliance" {
        & $PythonExe "scripts/check_spdx_headers.py"
        if ($LASTEXITCODE -ne 0) { throw "SPDX header check failed" }
        & $PythonExe "desktop/scripts/check_scriptscoreplus_boundary.py"
        if ($LASTEXITCODE -ne 0) { throw "ScriptScorePlus boundary check failed" }
        & $PythonExe "desktop/scripts/generate_legal_artifacts.py" --check
        if ($LASTEXITCODE -ne 0) { throw "legal artifact check failed" }
    }
}
finally {
    Pop-Location
}

Write-Host ""
Write-Host "Windows review-quality checks completed."
