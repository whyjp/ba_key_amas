# Checks the contract between the add-on release and the web server. Offline, no side effects.
# Usage: pwsh scripts/release-check.ps1 [-Tag v0.2.0] [-BaseRef origin/main] [-ImageBudgetMB 5]
# Fails (exit 1) when
#   - the add-on download URL the server hands out is not this repository's releases page,
#   - the tag is not v<workspace version>, or the version is lower than min_addon_version in release-contract.json,
#   - the add-on's upload protocol is outside the range the server accepts (web/src/version.ts),
#   - web/migrations is not the numbered sequence 0001, 0002, ... without gaps or repeats,
#   - a student lacks a file of the committed card art chain, or a JSON the Worker imports is missing,
#   - with -BaseRef, the change adds or rewrites more than -ImageBudgetMB (default 5) of images under web/public.
param([string]$Tag = "", [string]$BaseRef = "", [double]$ImageBudgetMB = 5)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$problems = New-Object System.Collections.Generic.List[string]
function Fail([string]$m) { $script:problems.Add($m) }
function Read-Text([string]$rel) { Get-Content -LiteralPath (Join-Path $root $rel) -Raw -Encoding UTF8 }
function Parse-Version([string]$s) {
    if ($s -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') { throw "not a version x.y.z: '$s'" }
    [version]($s -replace '-.*$', '')
}

$contract = (Read-Text "release-contract.json") | ConvertFrom-Json
$releases = "https://github.com/$($contract.repo)/releases"

# 1. download URL
if ($contract.addon_download_url -ne "$releases/latest") { Fail "release-contract.json addon_download_url '$($contract.addon_download_url)' is not $releases/latest" }
$wr = (Read-Text "web/wrangler.jsonc") -replace '(?m)^\s*//.*$', '' | ConvertFrom-Json
if ($wr.vars.ADDON_URL -ne $contract.addon_download_url) { Fail "web/wrangler.jsonc vars.ADDON_URL '$($wr.vars.ADDON_URL)' differs from the contract URL" }
$m = [regex]::Match((Read-Text "web/src/index.ts"), 'DEFAULT_ADDON_URL\s*=\s*"([^"]+)"')
if (-not $m.Success) { Fail "web/src/index.ts has no DEFAULT_ADDON_URL" }
elseif ($m.Groups[1].Value -ne $contract.addon_download_url) { Fail "web/src/index.ts DEFAULT_ADDON_URL '$($m.Groups[1].Value)' differs from the contract URL" }
if (Get-Command git -ErrorAction SilentlyContinue) {
    $remote = (git -C $root remote get-url origin 2>$null)
    if ($remote -and $remote -notmatch "github\.com[:/]$([regex]::Escape($contract.repo))(\.git)?$") { Fail "git remote origin '$remote' is not $($contract.repo)" }
}

# 2. version
$cargo = Read-Text "scanner/Cargo.toml"
$section = [regex]::Match($cargo, '(?ms)^\[workspace\.package\](.*?)(^\[|\z)').Groups[1].Value
$vm = [regex]::Match($section, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $vm.Success) { Fail "scanner/Cargo.toml has no [workspace.package] version" }
else {
    $ver = $vm.Groups[1].Value
    try {
        $min = Parse-Version $contract.min_addon_version
        if ((Parse-Version $ver) -lt $min) { Fail "add-on version $ver is lower than min_addon_version $($contract.min_addon_version)" }
        if ($Tag) {
            if ($Tag -notmatch '^v(\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?)$') { Fail "tag '$Tag' is not v<major>.<minor>.<patch>" }
            else {
                $tv = $Matches[1]
                if ($tv -ne $ver) { Fail "tag $Tag does not match scanner/Cargo.toml version $ver" }
                if ((Parse-Version $tv) -lt $min) { Fail "tag $Tag is lower than the server's min_addon_version $($contract.min_addon_version)" }
            }
        }
    } catch { Fail $_.Exception.Message }
}

# 3. upload protocol
$up = [regex]::Match((Read-Text "scanner/crates/cli/src/upload.rs"), 'pub const UPLOAD_PROTOCOL: u32 = (\d+);')
$vt = Read-Text "web/src/version.ts"
$pm = [regex]::Match($vt, 'export const PROTOCOL = (\d+);'); $mp = [regex]::Match($vt, 'export const MIN_PROTOCOL = (\d+);')
if (-not ($up.Success -and $pm.Success -and $mp.Success)) { Fail "could not read UPLOAD_PROTOCOL / PROTOCOL / MIN_PROTOCOL" }
else {
    $u = [int]$up.Groups[1].Value
    if ($u -lt [int]$mp.Groups[1].Value -or $u -gt [int]$pm.Groups[1].Value) { Fail "add-on UPLOAD_PROTOCOL $u is outside the server range $($mp.Groups[1].Value)..$($pm.Groups[1].Value)" }
}

# 4. migrations
$files = @(Get-ChildItem (Join-Path $root "web/migrations") -File | Sort-Object Name)
$n = 0
foreach ($f in $files) {
    $n++
    if ($f.Name -notmatch '^(\d{4})_[a-z0-9_]+\.sql$') { Fail "migration '$($f.Name)' is not NNNN_name.sql"; continue }
    if ([int]$Matches[1] -ne $n) { Fail "migration sequence breaks at '$($f.Name)': expected number $('{0:D4}' -f $n)" }
}
if ($files.Count -eq 0) { Fail "web/migrations is empty" }

# 5. committed assets: every student has the files the card art chain ends on, and the JSON the Worker imports exists
$catalogPath = Join-Path $root "web/src/catalog.json"
if (-not (Test-Path $catalogPath)) { Fail "web/src/catalog.json is missing (it is committed; see docs/ASSETS.md)" }
else {
    $ids = @((Get-Content $catalogPath -Raw -Encoding UTF8 | ConvertFrom-Json).PSObject.Properties.Name)
    foreach ($dir in "portraits", "icons", "card", "card-lobby") {
        $missing = @($ids | Where-Object { -not (Test-Path (Join-Path $root "web/public/$dir/$_.webp")) })
        if ($missing.Count) { Fail "web/public/$dir lacks $($missing.Count) of $($ids.Count) students, e.g. $($missing[0])" }
    }
    foreach ($f in "public/meta.json", "public/halos.json", "public/card-lobby.json", "public/faces.json", "src/backgrounds.json") { if (-not (Test-Path (Join-Path $root "web/$f"))) { Fail "web/$f is missing" } }
}

# 6. image size budget: a change may add or rewrite at most -ImageBudgetMB of images unless the PR is labelled
#    large-assets (CI sets ALLOW_LARGE_ASSETS=1) or ALLOW_LARGE_ASSETS=1 is set. Every re-commit stays in history for good.
if ($BaseRef) {
    git -C $root rev-parse --verify --quiet "$BaseRef^{commit}" *> $null
    if ($LASTEXITCODE -ne 0) { Write-Host "WARN: base ref '$BaseRef' not found, image budget not checked" -ForegroundColor Yellow }
    else {
        # one git call for the sizes: --raw gives the new blob ids, cat-file --batch-check turns them into byte counts
        $blobs = @(git -C $root diff --raw --no-renames --diff-filter=AM "$BaseRef...HEAD" -- web/public | Where-Object { $_ -match '\.(webp|png|jpe?g|gif)$' } | ForEach-Object { ($_ -split '\s+')[3] })
        $n = $blobs.Count
        $bytes = 0L
        if ($n) { $blobs | git -C $root cat-file --batch-check='%(objectsize)' | ForEach-Object { $bytes += [long]$_ } }
        $mb = [math]::Round($bytes / 1MB, 2)
        if ($bytes -gt $ImageBudgetMB * 1MB -and $env:ALLOW_LARGE_ASSETS -ne "1") { Fail "this change adds or rewrites $n image files, $mb MB, over the $ImageBudgetMB MB budget (label the PR large-assets if it is intended)" }
        else { Write-Host "images changed vs ${BaseRef}: $n files, $mb MB (budget $ImageBudgetMB MB)" }
    }
}

if ($problems.Count) { $problems | ForEach-Object { Write-Host "FAIL: $_" -ForegroundColor Red }; exit 1 }
"OK: url=$($contract.addon_download_url) version=$ver min=$($contract.min_addon_version) tag=$(if ($Tag) { $Tag } else { '-' }) protocol=$($up.Groups[1].Value) migrations=$($files.Count) students=$($ids.Count)"
