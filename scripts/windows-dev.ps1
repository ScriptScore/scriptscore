param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Command
)

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$WorkspaceRoot = Split-Path $RepoRoot -Parent
$ToolsRoot = Join-Path $WorkspaceRoot ".tools"

$GitBin = Join-Path $ToolsRoot "mingit\cmd"
$NodeRoot = Join-Path $ToolsRoot "node-v20.20.2-win-x64"
$UvRoot = Join-Path $ToolsRoot "uv"
$CargoRoot = Join-Path $ToolsRoot "cargo"
$RustupRoot = Join-Path $ToolsRoot "rustup"

$env:RUSTUP_HOME = $RustupRoot
$env:CARGO_HOME = $CargoRoot

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
        Write-Warning "vswhere.exe was not found; MSVC tools may be unavailable."
        return
    }

    $installPath = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (!$installPath) {
        Write-Warning "Visual Studio C++ Build Tools were not found; Rust MSVC builds may fail."
        return
    }

    $vsDevCmd = Join-Path $installPath "Common7\Tools\VsDevCmd.bat"
    if (!(Test-Path $vsDevCmd)) {
        Write-Warning "VsDevCmd.bat was not found at $vsDevCmd."
        return
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

Import-VsDevEnvironment
Add-PathFront (Join-Path $CargoRoot "bin")
Add-PathFront $UvRoot
Add-PathFront $NodeRoot
Add-PathFront $GitBin

Set-Location $RepoRoot

if ($Command.Count -gt 0) {
    & $Command[0] @($Command | Select-Object -Skip 1)
}
else {
    Write-Host "ScriptScore Windows dev environment loaded."
    Write-Host "Repo: $RepoRoot"
    Write-Host "Try: cargo test --manifest-path desktop/src-tauri/Cargo.toml --all-targets"
}
