# Scores the duty-list plate reading on the labelled plate pictures with the real Rust engine (offline, no game input).
# Usage: pwsh scripts/eval-plates.ps1 [-Exe D:\scan-run\ocr-plates\ba-key-AMAS.exe] [-Dir scanner\fixtures\plates]
param(
    [string]$Exe = "D:/cargo-target/baocr/release/ba-key-AMAS.exe",
    [string]$Dir = (Join-Path $PSScriptRoot "../scanner/fixtures/plates")
)
$ErrorActionPreference = "Stop"
if (-not $env:BAOCR_MODELS) { $env:BAOCR_MODELS = "D:/work-github/bluearchive-ocr/models" }
& $Exe --eval-plates (Resolve-Path $Dir)
if ($LASTEXITCODE -ne 0) { throw "eval-plates failed: exit $LASTEXITCODE" }
