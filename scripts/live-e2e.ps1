# Live end-to-end run against the running game. Usage: pwsh scripts/live-e2e.ps1 [-Label directml|cpu] [-Only nav,stale,tour,battle,card,guard,scan]
# Needs: cargo build --release -p baocr --features directml --example live_e2e ; cargo build --release -p baocr-hcu --examples
param([string]$Label = "directml", [string]$Only = "", [string]$Out = "D:\tmp\live-e2e")
$ErrorActionPreference = "Stop"
$t = "D:/cargo-target/baocr/release"
$env:BAOCR_MODELS = "D:/work-github/bluearchive-ocr/models"
$a = @("run", "--exe", "$t/ba-key-AMAS.exe", "--label", $Label, "--out", $Out)
if ($Only) { $a += @("--only", $Only) }
& "$t/examples/live_e2e.exe" @a
