# End to end check of the account model on the real artifacts: wrangler dev (the worker and a local D1 with all the
# migrations) and the built add-on exe. Made-up member codes only.
#
#   pwsh -File web/scripts/e2e-accounts.ps1 -Exe D:\scan-run\member-key\ba-key-AMAS.exe
#
# One wrangler instance at a time: stale wrangler and workerd processes are stopped first. Prints PASS or FAIL per check
# and exits 1 on any failure.
param(
    [string]$Exe = "D:\scan-run\member-key\ba-key-AMAS.exe",
    [string]$SigningKey = "e2e-signing-key-0123456789abcdef",
    [int]$Port = 8799,
    [string]$Work = "D:\scan-run\member-key\e2e"
)
$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$OutputEncoding = [System.Text.UTF8Encoding]::new()
$web = Split-Path -Parent $PSScriptRoot
$base = "http://127.0.0.1:$Port"
$state = Join-Path $Work "state"
$failed = 0

function Check([string]$name, [bool]$ok, [string]$detail = "") {
    if ($ok) { Write-Host "PASS  $name" } else { Write-Host "FAIL  $name  $detail"; $script:failed++ }
}

function AccountId([string]$code) {
    $norm = (($code.Trim() -replace '[^A-Za-z0-9]', '').ToUpperInvariant() -creplace 'O', '0') -creplace 'I', '1'
    $bytes = [System.Text.Encoding]::UTF8.GetBytes("ba-key-amas/account/v1`0" + $norm)
    -join ([System.Security.Cryptography.SHA256]::HashData($bytes) | ForEach-Object { $_.ToString("x2") })
}

function Api([string]$method, [string]$path, $body = $null, [hashtable]$headers = @{}) {
    $req = @{ Uri = "$base$path"; Method = $method; SkipHttpErrorCheck = $true; Headers = $headers }
    if ($null -ne $body) { $req.Body = ($body | ConvertTo-Json -Depth 10 -Compress); $req.ContentType = "application/json" }
    $r = Invoke-WebRequest @req
    [pscustomobject]@{ Status = [int]$r.StatusCode; Json = ($r.Content | ConvertFrom-Json -ErrorAction SilentlyContinue); Text = $r.Content }
}

function Upload([string]$code, [string[]]$more, [string]$key = $SigningKey) {
    $env:BAOCR_MEMBER_CODE = $code
    if ($key) { $env:BAOCR_ADDON_SIGNING_KEY = $key }
    $realAppData = $env:APPDATA
    $env:APPDATA = Join-Path $Work "appdata"
    $out = Join-Path $Work "upload.out"
    $err = Join-Path $Work "upload.err"
    $p = Start-Process -FilePath $Exe -ArgumentList (@("--upload", $base, "--out", (Join-Path $Work "students")) + $more) -Wait -PassThru -NoNewWindow -RedirectStandardOutput $out -RedirectStandardError $err
    Remove-Item Env:BAOCR_MEMBER_CODE
    Remove-Item Env:BAOCR_ADDON_SIGNING_KEY -ErrorAction SilentlyContinue
    $env:APPDATA = $realAppData
    [pscustomobject]@{ Exit = $p.ExitCode; Text = ((Get-Content $out -Raw -ErrorAction SilentlyContinue) + (Get-Content $err -Raw -ErrorAction SilentlyContinue)) }
}

# HMAC-SHA256(key, "ts\npath\nsha256(body)") in hex: what the add-on adds to every request that changes an account
function SignedApi([string]$path, $body, [string]$key = $SigningKey, [hashtable]$extra = @{}) {
    $json = ($body | ConvertTo-Json -Depth 10 -Compress)
    $ts = [string][DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $digest = -join ([System.Security.Cryptography.SHA256]::HashData([System.Text.Encoding]::UTF8.GetBytes($json)) | ForEach-Object { $_.ToString("x2") })
    $hmac = [System.Security.Cryptography.HMACSHA256]::new([System.Text.Encoding]::UTF8.GetBytes($key))
    $sig = -join ($hmac.ComputeHash([System.Text.Encoding]::UTF8.GetBytes("$ts`n$path`n$digest")) | ForEach-Object { $_.ToString("x2") })
    $h = @{ "x-amas-protocol" = "2"; "x-amas-addon" = "e2e"; "x-amas-ts" = $ts; "x-amas-sig" = $sig } + $extra
    $r = Invoke-WebRequest -Uri "$base$path" -Method POST -Body $json -ContentType "application/json" -Headers $h -SkipHttpErrorCheck
    [pscustomobject]@{ Status = [int]$r.StatusCode; Json = ($r.Content | ConvertFrom-Json -ErrorAction SilentlyContinue); Text = $r.Content }
}

function Sql([string]$sql) {
    Push-Location $web
    try { npx wrangler d1 execute ba-key-amas --local --persist-to $state --json --command $sql 2>$null | Out-String } finally { Pop-Location }
}

function Rows() { (Sql "SELECT id, nickname, public, public_id FROM users ORDER BY id" | ConvertFrom-Json)[0].results }

function StopWrangler() {
    Get-CimInstance Win32_Process | Where-Object { $_.Name -eq "workerd.exe" -or ($_.Name -eq "node.exe" -and $_.CommandLine -match "wrangler") } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

# a clean slate: no stale wrangler or workerd, an empty state and an empty add-on folder
StopWrangler
if (Test-Path $Work) { Remove-Item $Work -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $Work "students"), (Join-Path $Work "appdata\baocr\progress") | Out-Null
@{ schema_version = 3; entries = @(
    @{ student = @{ name = "아루"; level = 80; rarity_stars = 3 } },
    @{ student = @{ name = "하루"; level = 70; rarity_stars = 2 } }) } | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $Work "students\students.json") -Encoding utf8
@{ schema_version = 2; account = @{ nickname = "소다팝" } } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $Work "appdata\baocr\progress\latest.json") -Encoding utf8
foreach ($f in @(@("catalog.json", "{}"), @("backgrounds.json", "[]"))) { $p = Join-Path $web "src\$($f[0])"; if (-not (Test-Path $p)) { Set-Content $p $f[1] } }
Set-Content (Join-Path $web ".dev.vars") "KEY_PEPPER=e2e-only-pepper`nADDON_SIGNING_KEY=$SigningKey"

Push-Location $web
npx wrangler d1 migrations apply ba-key-amas --local --persist-to $state 2>&1 | Out-Null
$log = Join-Path $Work "wrangler.log"
$dev = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", "npx wrangler dev --port $Port --persist-to `"$state`" > `"$log`" 2>&1" -PassThru -WindowStyle Hidden
Pop-Location
try {
    $up = $false
    for ($i = 0; $i -lt 60 -and -not $up; $i++) {
        Start-Sleep -Seconds 1
        try { $up = (Invoke-WebRequest "$base/api/version" -SkipHttpErrorCheck).StatusCode -eq 200 } catch { }
    }
    Check "wrangler dev answers /api/version" $up
    if (-not $up) { throw "server did not start; see $log" }

    $codeA = "ab12cd34ef56g"
    $codeB = "ZX98YW76VU54T"
    $idA = AccountId $codeA
    $idB = AccountId $codeB

    $r1 = Upload $codeA @("--public")
    Check "first upload creates an account" ($r1.Exit -eq 0 -and $r1.Text -match "new account") $r1.Text
    $rows = @(Rows)
    Check "one row after the first upload" ($rows.Count -eq 1)
    $pid1 = $rows[0].public_id

    $r2 = Upload $codeA @("--nickname", "새이름")
    Check "same member code again updates the account" ($r2.Exit -eq 0 -and $r2.Text -match "updated" -and $r2.Text -match "public") $r2.Text
    $rows = @(Rows)
    Check "still one row, nickname refreshed, public kept, same public_id" ($rows.Count -eq 1 -and $rows[0].nickname -eq "새이름" -and $rows[0].public -eq 1 -and $rows[0].public_id -eq $pid1)
    Write-Host ($rows | ConvertTo-Json -Compress)

    $r3 = Upload $codeB @("--nickname", "새이름", "--public")
    Check "another member code with the same nickname creates a second account" ($r3.Exit -eq 0 -and $r3.Text -match "new account") $r3.Text
    $rows = @(Rows)
    Check "two rows share a nickname" ($rows.Count -eq 2 -and $rows[0].nickname -eq $rows[1].nickname -and $rows[0].public_id -ne $rows[1].public_id)
    Write-Host ($rows | ConvertTo-Json -Compress)

    $s = Api GET ("/api/search?q=" + [uri]::EscapeDataString("새이"))
    Check "search lists both as candidates" ($s.Status -eq 200 -and @($s.Json.matches).Count -eq 2) $s.Text
    Write-Host $s.Text

    $nick = Api GET ("/api/public?id=" + [uri]::EscapeDataString("새이름"))
    Check "a nickname is not a way to open a profile" ($nick.Status -eq 404)
    $page = Api GET "/api/public?id=$pid1"
    Check "a profile opens by public_id" ($page.Status -eq 200 -and $page.Json.nickname -eq "새이름")

    $forged = Api POST "/api/visibility" @{ account_id = $idA; public = $false } @{ "x-amas-protocol" = "2"; "x-amas-addon" = "e2e" }
    Check "the add-on headers alone are not enough: no signature gets 401" ($forged.Status -eq 401) $forged.Text
    $wrong = SignedApi "/api/visibility" @{ account_id = $idA; public = $false } "some-other-key-0123456789abcdef"
    Check "a signature made with another key gets 401" ($wrong.Status -eq 401) $wrong.Text
    $stillListed = Api GET ("/api/search?q=" + [uri]::EscapeDataString("새이"))
    Check "the refused requests changed nothing: both accounts are still public and listed" (@($stillListed.Json.matches).Count -eq 2) $stillListed.Text
    $unsignedBuild = Upload $codeA @() ""
    Check "an add-on build without the signing key cannot upload (exit code is not 0, the message says why)" ($unsignedBuild.Exit -ne 0 -and $unsignedBuild.Text -match "signing key") $unsignedBuild.Text
    $h = SignedApi "/api/visibility" @{ account_id = $idA; public = $false }
    Check "visibility by account_id (signed like the add-on)" ($h.Status -eq 200 -and $h.Json.public -eq $false) $h.Text
    $s2 = Api GET ("/api/search?q=" + [uri]::EscapeDataString("새이"))
    Check "a private account is no longer a candidate but its link still opens" (@($s2.Json.matches).Count -eq 1 -and (Api GET "/api/public?id=$pid1").Status -eq 200)

    $old = Api POST "/api/upload" @{ account_id = $idA; nickname = "x"; roster = @{} } @{ "x-amas-protocol" = "1"; "x-amas-addon" = "0.1.0" }
    Check "protocol 1 gets 426 and the download link" ($old.Status -eq 426 -and $old.Json.download -like "https://*") $old.Text
    $bad = SignedApi "/api/visibility" @{ account_id = "nope"; public = $true }
    Check "a malformed account_id is refused" ($bad.Status -eq 400)

    $saved = Get-Content (Join-Path $Work "appdata\baocr\upload.json") -Raw -ErrorAction SilentlyContinue
    Check "scripted codes are not saved to upload.json" ([string]::IsNullOrEmpty($saved))

    Start-Sleep -Seconds 1
    $logText = (Get-Content $log -Raw) + $r1.Text + $r2.Text + $r3.Text + $s.Text + $s2.Text + $page.Text + $h.Text + $old.Text + $bad.Text + $forged.Text + $wrong.Text + $unsignedBuild.Text
    $leak = @($codeA, $codeB, $idA, $idB) | Where-Object { $logText -match [regex]::Escape($_) }
    Check "no member code or account_id in the server log, any response or the add-on output" (@($leak).Count -eq 0) ($leak -join ",")
    $db = (Sql "SELECT * FROM users") + (Sql "SELECT * FROM students")
    Check "no account_id or member code in any column of users or students" ($db.Length -gt 200 -and $db -notmatch $idA -and $db -notmatch $idB -and $db -notmatch $codeA -and $db -notmatch $codeB)
} finally {
    StopWrangler
    if ($dev -and -not $dev.HasExited) { Stop-Process -Id $dev.Id -Force -ErrorAction SilentlyContinue }
}
if ($failed -gt 0) { Write-Host "FAILED: $failed check(s)"; exit 1 }
Write-Host "ALL PASSED"
