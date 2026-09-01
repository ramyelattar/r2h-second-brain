<#
.SYNOPSIS
  Verifies essential payload file hashes against payload-manifest.json.
  Used by verify-install.ps1 and by the installer's transactional gate.
  Exit 0 = all essential files present and matching, 1 = mismatch/missing.
#>
param(
    [string]$ManifestPath = "C:\ProgramData\R2H.AI-ELE\payload-manifest.json",
    [string]$JsonOut
)

$ErrorActionPreference = "Stop"

function Get-SHA256 {
    param([string]$Path)
    $stream = [IO.File]::OpenRead($Path)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace("-", "").ToLower() }
        finally { $sha.Dispose() }
    }
    finally { $stream.Dispose() }
}

$PackRoot = "C:\ProgramData\R2H.AI-ELE"
$AppHome = "C:\ProgramData\R2H\r2h-second-brain"
$AppDir = "C:\Program Files\R2H\r2h-second-brain"

$payload = Get-Content $ManifestPath -Raw | ConvertFrom-Json
$bad = 0
$checked = 0
foreach ($file in $payload.files) {
    if (-not $file.essential) { continue }
    $rootPath = $null
    if ($file.root -eq "pack") { $rootPath = $PackRoot }
    if ($file.root -eq "appHome") { $rootPath = $AppHome }
    if ($file.root -eq "app") { $rootPath = $AppDir }
    if ($null -eq $rootPath) { $bad++; continue }
    $full = $rootPath + "\" + ($file.path -replace "/", "\")
    if (-not (Test-Path $full)) { $bad++; continue }
    $hash = Get-SHA256 $full
    $checked++
    if ($hash -ne ([string]$file.sha256).ToLower()) { $bad++ }
}

if ($JsonOut) {
    [ordered]@{
        manifest = $ManifestPath
        checked  = $checked
        bad      = $bad
        overall  = $(if ($bad -eq 0) { "PASS" } else { "FAIL" })
    } | ConvertTo-Json | Set-Content -LiteralPath $JsonOut -Encoding utf8
}
Write-Host ("PAYLOAD_HASH_CHECK checked={0} bad={1}" -f $checked, $bad)
exit $(if ($bad -eq 0) { 0 } else { 1 })
