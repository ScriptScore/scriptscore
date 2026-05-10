# SPDX-License-Identifier: AGPL-3.0-only

param(
    [string] $DestDir = $env:PADDLE_MODEL_ROOT
)

$ErrorActionPreference = "Stop"

$DetUrl = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_mobile_det_infer.tar"
$RecUrl = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_mobile_rec_infer.tar"

if (!$DestDir) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $CliDir = Resolve-Path (Join-Path $ScriptDir "..")
    $DestDir = Join-Path $CliDir "models\paddle"
}

function Download-File {
    param(
        [string] $Url,
        [string] $OutFile
    )

    $attempt = 1
    while ($attempt -le 3) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $OutFile
            return
        }
        catch {
            if ($attempt -ge 3) {
                throw
            }
            Start-Sleep -Seconds 2
            $attempt += 1
        }
    }
}

function Assert-ModelDir {
    param([string] $ModelDir)

    if (!(Test-Path (Join-Path $ModelDir "inference.yml"))) {
        throw "model install incomplete: missing $ModelDir\inference.yml"
    }
    if (!(Test-Path (Join-Path $ModelDir "inference.pdmodel")) -and !(Test-Path (Join-Path $ModelDir "inference.json"))) {
        throw "model install incomplete: missing $ModelDir\inference.pdmodel or $ModelDir\inference.json"
    }
    if (!(Test-Path (Join-Path $ModelDir "inference.pdiparams"))) {
        throw "model install incomplete: missing $ModelDir\inference.pdiparams"
    }
}

function Find-ExtractedModelDir {
    param(
        [string] $TempDir,
        [string] $ModelName
    )

    $marker = Get-ChildItem -Path $TempDir -Recurse -Filter "inference.yml" |
        Where-Object { $_.FullName -like "*$ModelName*" } |
        Select-Object -First 1
    if ($null -eq $marker) {
        throw "failed to find extracted Paddle model directory: $ModelName"
    }
    Split-Path -Parent $marker.FullName
}

if (!(Get-Command tar.exe -ErrorAction SilentlyContinue)) {
    throw "tar.exe was not found. Install Windows tar support or run this script from an environment that provides tar."
}

$DestRoot = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($DestDir)
$DetDest = Join-Path $DestRoot "det"
$RecDest = Join-Path $DestRoot "rec"
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("scriptscore-paddle-models-" + [guid]::NewGuid())

New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

try {
    $DetTar = Join-Path $TempDir "det.tar"
    $RecTar = Join-Path $TempDir "rec.tar"

    Write-Host "Downloading Paddle detector model..."
    Download-File -Url $DetUrl -OutFile $DetTar

    Write-Host "Downloading Paddle recognizer model..."
    Download-File -Url $RecUrl -OutFile $RecTar

    Write-Host "Extracting archives..."
    & tar.exe -xf $DetTar -C $TempDir
    & tar.exe -xf $RecTar -C $TempDir

    $DetSrc = Find-ExtractedModelDir -TempDir $TempDir -ModelName "PP-OCRv5_mobile_det_infer"
    $RecSrc = Find-ExtractedModelDir -TempDir $TempDir -ModelName "PP-OCRv5_mobile_rec_infer"

    New-Item -ItemType Directory -Force -Path $DetDest, $RecDest | Out-Null

    Write-Host "Installing detector into $DetDest"
    Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath `
        (Join-Path $DetDest "inference.json"), `
        (Join-Path $DetDest "inference.pdmodel"), `
        (Join-Path $DetDest "inference.pdiparams"), `
        (Join-Path $DetDest "inference.pdiparams.info"), `
        (Join-Path $DetDest "inference.yml")
    Copy-Item -Path (Join-Path $DetSrc "*") -Destination $DetDest -Force

    Write-Host "Installing recognizer into $RecDest"
    Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath `
        (Join-Path $RecDest "inference.json"), `
        (Join-Path $RecDest "inference.pdmodel"), `
        (Join-Path $RecDest "inference.pdiparams"), `
        (Join-Path $RecDest "inference.pdiparams.info"), `
        (Join-Path $RecDest "inference.yml")
    Copy-Item -Path (Join-Path $RecSrc "*") -Destination $RecDest -Force

    Assert-ModelDir -ModelDir $DetDest
    Assert-ModelDir -ModelDir $RecDest

    Write-Host ""
    Write-Host "PaddleOCR models installed successfully."
    Write-Host "Model root: $DestRoot"
    Write-Host ""
    Write-Host "For CLI tests:"
    Write-Host "  `$env:SCRIPTSCORE_TEST_PII_PADDLE_MODEL_DIR = `"$DestRoot`""
}
finally {
    if (Test-Path $TempDir) {
        Remove-Item -Recurse -Force -LiteralPath $TempDir
    }
}
