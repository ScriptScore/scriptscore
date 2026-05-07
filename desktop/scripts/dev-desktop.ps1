# SPDX-License-Identifier: AGPL-3.0-only

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$DesktopDir = Resolve-Path (Join-Path $ScriptDir "..")
$RepoRoot = Resolve-Path (Join-Path $DesktopDir "..")
$WorkspaceRoot = Split-Path $RepoRoot -Parent
$ToolsRoot = Join-Path $WorkspaceRoot ".tools"
$FrontendDir = Join-Path $DesktopDir "frontend"
$HostDir = Join-Path $DesktopDir "src-tauri"
$CargoToml = Join-Path $HostDir "Cargo.toml"
$HostExe = Join-Path $HostDir "target\debug\scriptscore-desktop-host.exe"
$DevHost = "127.0.0.1"
$DevPort = "5173"
$DevUrl = "http://${DevHost}:${DevPort}"
$WaitSeconds = 30
$StartedFrontend = $null
$HostProcess = $null

function Add-PathFront {
    param([string] $PathEntry)

    if (Test-Path $PathEntry) {
        $entries = $env:PATH -split ";" | Where-Object { $_ -and $_ -ne $PathEntry }
        $env:PATH = (@($PathEntry) + $entries) -join ";"
    }
}

function Import-VsDevEnvironment {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (!(Test-Path $vswhere)) {
        throw "vswhere.exe was not found. Install Visual Studio Build Tools with the C++ workload."
    }

    $installPath = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (!$installPath) {
        throw "Visual Studio C++ Build Tools were not found. Install the C++ build tools workload, then rerun this script."
    }

    $vsDevCmd = Join-Path $installPath "Common7\Tools\VsDevCmd.bat"
    if (!(Test-Path $vsDevCmd)) {
        throw "VsDevCmd.bat was not found at $vsDevCmd."
    }

    $envLines = cmd.exe /c "`"$vsDevCmd`" -arch=x64 >nul && set"
    foreach ($line in $envLines) {
        $name, $value = $line -split "=", 2
        if ($name -and $value) {
            Set-Item -Path "Env:$name" -Value $value
        }
    }

    # Avoid loading Visual Studio API-set shims ahead of the operating system DLLs.
    $env:PATH = (($env:PATH -split ";") | Where-Object {
        $_ -and $_ -notlike "*Microsoft Visual Studio*Common7*IDE*"
    }) -join ";"
}

function Initialize-ToolPath {
    $cargoRoot = Join-Path $ToolsRoot "cargo"
    $rustupRoot = Join-Path $ToolsRoot "rustup"
    $nodeRoot = Join-Path $ToolsRoot "node-v20.20.2-win-x64"
    $uvRoot = Join-Path $ToolsRoot "uv"
    $gitBin = Join-Path $ToolsRoot "mingit\cmd"

    if (Test-Path $cargoRoot) {
        $env:CARGO_HOME = $cargoRoot
    }
    if (Test-Path $rustupRoot) {
        $env:RUSTUP_HOME = $rustupRoot
    }

    Add-PathFront (Join-Path $cargoRoot "bin")
    Add-PathFront $uvRoot
    Add-PathFront $nodeRoot
    Add-PathFront $gitBin
}

function Assert-DefaultDevUrl {
    if ($env:DESKTOP_FRONTEND_URL -and $env:DESKTOP_FRONTEND_URL -ne $DevUrl) {
        throw "desktop/scripts/dev-desktop.ps1 uses the fixed Tauri dev URL $DevUrl. Unset DESKTOP_FRONTEND_URL or set it to $DevUrl."
    }
    if ($env:VITE_HOST -and $env:VITE_HOST -ne $DevHost) {
        throw "desktop/scripts/dev-desktop.ps1 uses fixed Vite host $DevHost. Unset VITE_HOST or set it to $DevHost."
    }
    if ($env:VITE_PORT -and $env:VITE_PORT -ne $DevPort) {
        throw "desktop/scripts/dev-desktop.ps1 uses fixed Vite port $DevPort. Unset VITE_PORT or set it to $DevPort."
    }
}

function Assert-Prerequisites {
    if (!(Get-Command npm -ErrorAction SilentlyContinue)) {
        throw "npm was not found. Install Node.js/npm or run the ScriptScore Windows tool setup, then rerun this script."
    }
    if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo was not found. Install Rust/Cargo or run the ScriptScore Windows tool setup, then rerun this script."
    }
    if (!(Test-Path (Join-Path $FrontendDir "package.json"))) {
        throw "Could not find desktop/frontend/package.json."
    }
    if (!(Test-Path (Join-Path $FrontendDir "node_modules"))) {
        throw "Frontend dependencies are missing. Run: cd $FrontendDir; npm ci"
    }
    if (!(Test-Path $CargoToml)) {
        throw "Could not find desktop/src-tauri/Cargo.toml."
    }
}

function Test-FrontendReady {
    try {
        $response = Invoke-WebRequest -UseBasicParsing -Uri $DevUrl -TimeoutSec 2
        return $response.StatusCode -ge 200
    }
    catch {
        return $false
    }
}

function Wait-ForFrontend {
    $deadline = (Get-Date).AddSeconds($WaitSeconds)
    while ((Get-Date) -lt $deadline) {
        if (Test-FrontendReady) {
            return
        }
        Start-Sleep -Milliseconds 500
    }
    throw "Frontend dev server did not become ready at $DevUrl within ${WaitSeconds}s."
}

function Stop-ProcessTree {
    param([System.Diagnostics.Process] $Process)

    if ($null -eq $Process -or $Process.HasExited) {
        return
    }
    taskkill.exe /PID $Process.Id /T /F | Out-Null
}

function Start-DesktopHost {
    Write-Host "Launching ScriptScore Desktop..."
    $existing = Get-Process -Name "scriptscore-desktop-host" -ErrorAction SilentlyContinue
    foreach ($process in $existing) {
        Stop-Process -Id $process.Id -Force
    }
    Start-Process -FilePath explorer.exe -ArgumentList @($HostExe) | Out-Null
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline) {
        $candidate = Get-Process -Name "scriptscore-desktop-host" -ErrorAction SilentlyContinue |
            Where-Object { $_.MainWindowHandle -ne 0 } |
            Select-Object -First 1
        if ($null -ne $candidate) {
            return $candidate
        }
        Start-Sleep -Milliseconds 500
    }
    throw "ScriptScore desktop host started, but no visible window was detected."
}

function Cleanup {
    if ($null -ne $StartedFrontend) {
        Write-Host "Stopping frontend dev server..."
        Stop-ProcessTree -Process $StartedFrontend
    }
}

Initialize-ToolPath
Import-VsDevEnvironment
Assert-DefaultDevUrl
Assert-Prerequisites

try {
    if (Test-FrontendReady) {
        Write-Host "Frontend is already responding at $DevUrl; reusing it."
    }
    else {
        Write-Host "Starting frontend dev server at $DevUrl..."
        $StartedFrontend = Start-Process -FilePath "npm.cmd" `
            -ArgumentList @("run", "dev", "--", "--host", $DevHost, "--port", $DevPort) `
            -WorkingDirectory $FrontendDir `
            -NoNewWindow `
            -PassThru
        Wait-ForFrontend
    }

    Write-Host "Building ScriptScore desktop host..."
    Push-Location $RepoRoot
    try {
        cargo build --manifest-path "desktop\src-tauri\Cargo.toml"
    }
    finally {
        Pop-Location
    }

    $HostProcess = Start-DesktopHost
    Write-Host "ScriptScore Desktop is running. Close the app window to stop this launcher."
    Wait-Process -Id $HostProcess.Id
}
finally {
    Cleanup
}
