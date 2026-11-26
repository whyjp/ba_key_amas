# Builds the portable add-on folder: one exe with everything it needs beside it, then a zip and SHA256SUMS.
# Usage: pwsh scripts/package-portable.ps1 [-Out D:\dist\ba-scanner] [-Zip D:\dist\ba-key-AMAS-v0.1.0-windows-x64.zip] [-SkipBuild]
# Needs models\ and wikidb\wiki.db, both committed seeds (docs/SEED-DATA.md).
# CARGO_HOME / CARGO_TARGET_DIR are honoured when set; otherwise cargo's defaults are used.
param([string]$Out = "D:\dist\ba-scanner", [string]$Zip = "", [switch]$SkipBuild)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
function Need([string]$p) { if (-not (Test-Path $p)) { throw "missing $p" } }
Need "$root\wikidb\wiki.db"
foreach ($f in "PP-OCRv6_det_small.onnx", "korean_PP-OCRv5_rec_mobile.onnx", "korean_PP-OCRv5_dict.txt") { Need "$root\models\$f" }

Push-Location "$root\scanner"
try {
    if (-not $SkipBuild) {
        cargo build --release -p baocr --features directml
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
    }
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$root\scanner\target" }
} finally { Pop-Location }
$rel = Join-Path $target "release"
Need "$rel\ba-key-AMAS.exe"

if (Test-Path $Out) { Remove-Item $Out -Recurse -Force }
New-Item -ItemType Directory -Force $Out, "$Out\models", "$Out\wikidb" | Out-Null
Copy-Item "$rel\ba-key-AMAS.exe" "$Out\ba-key-AMAS.exe" -Force
Get-ChildItem $rel -Filter *.dll | Copy-Item -Destination $Out -Force
Copy-Item "$root\models\*" "$Out\models" -Recurse -Force
Copy-Item "$root\wikidb\wiki.db" "$Out\wikidb" -Force
foreach ($f in "README.md", "THIRD_PARTY_NOTICES.md") { if (Test-Path "$root\$f") { Copy-Item "$root\$f" $Out -Force } }
# TrOCR stays in the repository only (models-optional/trocr, docs/trocr-evaluation.md)
if (Get-ChildItem $Out -Recurse -File | Where-Object { $_.Name -match "trocr|^(encoder|decoder)\.onnx$" }) { throw "TrOCR files must not be packaged" }
"{0:N1} MB in {1}" -f ((Get-ChildItem $Out -Recurse -File | Measure-Object Length -Sum).Sum / 1MB), $Out

if ($Zip) {
    $zipDir = Split-Path $Zip -Parent
    if ($zipDir) { New-Item -ItemType Directory -Force $zipDir | Out-Null }
    if (Test-Path $Zip) { Remove-Item $Zip -Force }
    Compress-Archive -Path "$Out\*" -DestinationPath $Zip -CompressionLevel Optimal
    $hash = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLower()
    $sums = Join-Path $zipDir "SHA256SUMS"
    "$hash  $(Split-Path $Zip -Leaf)" | Set-Content $sums -Encoding ascii
    "{0:N1} MB  sha256 {1}" -f ((Get-Item $Zip).Length / 1MB), $hash
}
