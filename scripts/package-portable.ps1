# Builds the portable add-on folder: one exe with everything it needs beside it.
# Usage: pwsh scripts/package-portable.ps1 [-Out D:\dist\ba-scanner]
param([string]$Out = "D:\dist\ba-scanner")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$env:CARGO_HOME = "D:/cargo-home"; $env:CARGO_TARGET_DIR = "D:/cargo-target/baocr"
Push-Location "$root\rust"; cargo build --release -p baocr --features directml; Pop-Location
$rel = "$env:CARGO_TARGET_DIR/release"
New-Item -ItemType Directory -Force $Out, "$Out\models", "$Out\wikidb", "$Out\vendor\hcu" | Out-Null
Copy-Item "$rel\baocr.exe" "$Out\ba-scanner.exe" -Force
Get-ChildItem $rel -Filter *.dll | Copy-Item -Destination $Out -Force
Copy-Item "$root\models\*" "$Out\models" -Recurse -Force
Copy-Item "$root\wikidb\wiki.db" "$Out\wikidb" -Force
Copy-Item "$root\vendor\hcu\*" "$Out\vendor\hcu" -Recurse -Force
"{0:N1} MB in {1}" -f ((Get-ChildItem $Out -Recurse -File | Measure-Object Length -Sum).Sum / 1MB), $Out
