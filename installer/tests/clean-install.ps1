<#
.SYNOPSIS
  Phase 12 clean-machine install test driver.
  1. Stops all dev/product processes.
  2. Neutralizes pre-existing state (pack, junction, registry, legacy uninstaller).
  3. Runs the installer silently (offline provisioning path).
  4. Verifies installed layout.
#>
param(
    [Parameter(Mandatory)][string]$InstallerPath,
    [string]$LogPath = "$env:TEMP\r2h-install-test.log"
)

$ErrorActionPreference = "Stop"

Write-Host "===== STEP 1: stop all product/dev processes ====="
& "E:\Projects\R2H-Desktop\01-R2H-PRODUCTS\r2h-second-brain\installer\tests\stop-product-processes.ps1" -IncludeDevelopment

Write-Host "===== STEP 2: neutralize pre-existing state ====="
# 2a. Retire legacy uninstaller + registry entry (simulates their absence on a
# clean machine; the installer must tolerate absence as well as presence).
$legacyRoot = "C:\ProgramData\R2H.AI-ELE"
foreach ($f in @("unins000.exe", "unins000.dat", "unins000.msg")) {
    $p = Join-Path $legacyRoot $f
    if (Test-Path $p) { Remove-Item -Force $p; Write-Host "removed $p" }
}
foreach ($key in @(
    "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{DC2B553F-96D8-4FA4-9273-A9E4E49F090D}_is1",
    "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{DC2B553F-96D8-4FA4-9273-A9E4E49F090D}_is1")) {
    if (Test-Path $key) { Remove-Item -Recurse -Force $key; Write-Host "removed $key" }
}

# 2b. Move the live AI pack aside (clean machine has no pack). Staging pack in
# release-staging remains the recovery source.
if (Test-Path "$legacyRoot\local-ai") {
    if (Test-Path "C:\ProgramData\_R2H.AI-ELE.devbackup") {
        Remove-Item -Recurse -Force "C:\ProgramData\_R2H.AI-ELE.devbackup"
    }
    Rename-Item "$legacyRoot\local-ai" "_devbackup-local-ai"
    Move-Item "$legacyRoot\_devbackup-local-ai" "C:\ProgramData\_R2H.AI-ELE.devbackup"
    Write-Host "moved live pack to C:\ProgramData\_R2H.AI-ELE.devbackup"
}
foreach ($f in @("local-ai-pack.json", "payload-manifest.json")) {
    $p = Join-Path $legacyRoot $f
    if (Test-Path $p) { Move-Item $p "$p.devbackup" -Force; Write-Host "moved $p" }
}

# 2c. Remove the staging junction (clean machine has no app home).
$junction = "C:\ProgramData\R2H\r2h-second-brain"
if (Test-Path $junction) {
    $item = Get-Item $junction -Force
    if ($item.LinkType -eq "Junction") {
        cmd /c rmdir "$junction" | Out-Null
        Write-Host "removed junction $junction"
    }
    else {
        throw "$junction exists and is not a junction; refusing to delete unknown data"
    }
}
if ((Test-Path "C:\ProgramData\R2H") -and -not (Get-ChildItem "C:\ProgramData\R2H")) {
    Remove-Item "C:\ProgramData\R2H"; Write-Host "removed empty C:\ProgramData\R2H"
}

# 2d. Remove machine env var if present (clean machine).
$envKey = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment"
if (Get-ItemProperty -Path $envKey -Name "R2H_SECOND_BRAIN_PROJECT_ROOT" -ErrorAction SilentlyContinue) {
    Remove-ItemProperty -Path $envKey -Name "R2H_SECOND_BRAIN_PROJECT_ROOT"
    Write-Host "removed machine env var"
}

# 2e. Ensure no leftover app data (clean machine).
$appData = "$env:APPDATA\ai.r2h.second-brain"
if (Test-Path $appData) { Write-Host "WARNING: $appData exists; test proceeds (data survival is checked in upgrade phase)" }

Write-Host "===== STEP 3: silent install ====="
$proc = Start-Process -FilePath $InstallerPath -ArgumentList "/VERYSILENT", "/NORESTART", "/SUPPRESSMSGBOXES", "/LOG=$LogPath" -PassThru -Wait
Write-Host ("installer exit code: {0}" -f $proc.ExitCode)
if ($proc.ExitCode -ne 0) {
    Get-Content $LogPath -Tail 40
    throw "installer failed with exit code $($proc.ExitCode)"
}

Write-Host "===== STEP 4: verify installed layout ====="
$checks = [ordered]@{
    "app exe"            = "C:\Program Files\R2H\r2h-second-brain\r2h-second-brain-app.exe"
    "verify tool"        = "C:\Program Files\R2H\r2h-second-brain\tools\verify-install.ps1"
    "pack manifest"      = "C:\ProgramData\R2H.AI-ELE\local-ai\manifests\r2h-multi-evidence-models.json"
    "llama-server"       = "C:\ProgramData\R2H.AI-ELE\local-ai\runtimes\llama-server.exe"
    "generation gguf"    = "C:\ProgramData\R2H.AI-ELE\local-ai\models\generation\qwen3-4b-q4km-generation.gguf"
    "pack python"        = "C:\ProgramData\R2H.AI-ELE\local-ai\python-runtime\python.exe"
    "reranker worker"    = "C:\ProgramData\R2H.AI-ELE\local-ai\workers\reranker_worker.py"
    "payload manifest"   = "C:\ProgramData\R2H.AI-ELE\payload-manifest.json"
    "khoj venv python"   = "C:\ProgramData\R2H\r2h-second-brain\engines\khoj\.venv\Scripts\python.exe"
    "khoj base python"   = "C:\ProgramData\R2H\r2h-second-brain\engines\khoj\base-python311\python.exe"
    "khoj wrapper"       = "C:\ProgramData\R2H\r2h-second-brain\scripts\run-khoj-windows.py"
    "run-data dir"       = "C:\ProgramData\R2H\r2h-second-brain\run-data"
    "no staging left"    = $null
}
$failedLayout = 0
foreach ($name in $checks.Keys) {
    $path = $checks[$name]
    if ($name -eq "no staging left") {
        $leftover = @(Get-ChildItem "C:\ProgramData\R2H.AI-ELE" -Directory -Filter "staging-*" -ErrorAction SilentlyContinue) +
            @(Get-ChildItem "C:\ProgramData\R2H\r2h-second-brain" -Directory -Filter "staging-*" -ErrorAction SilentlyContinue)
        if ($leftover.Count -ne 0) { Write-Host "FAIL $name ($($leftover.Count) staging dirs left)"; $failedLayout++; }
        else { Write-Host "PASS $name" }
        continue
    }
    if (Test-Path $path) { Write-Host "PASS $name" } else { Write-Host "FAIL $name ($path)"; $failedLayout++ }
}
# legacy artifacts must be gone and stay gone
if (Test-Path "$legacyRoot\unins000.exe") { Write-Host "FAIL legacy uninstaller restored"; $failedLayout++ } else { Write-Host "PASS legacy uninstaller retired" }

# env var set
$envVal = (Get-ItemProperty -Path $envKey -Name "R2H_SECOND_BRAIN_PROJECT_ROOT" -ErrorAction SilentlyContinue).R2H_SECOND_BRAIN_PROJECT_ROOT
if ($envVal -eq "C:\ProgramData\R2H\r2h-second-brain") { Write-Host "PASS env var" } else { Write-Host "FAIL env var ($envVal)"; $failedLayout++ }

# run-data ACL: Users modify
$acl = (Get-Acl "C:\ProgramData\R2H\r2h-second-brain\run-data").Access |
    Where-Object { $_.IdentityReference -like "*Users*" -and $_.FileSystemRights -match "Modify" }
if ($acl) { Write-Host "PASS run-data ACL (Users modify)" } else { Write-Host "FAIL run-data ACL"; $failedLayout++ }

Write-Host ""
if ($failedLayout -eq 0) { Write-Host "CLEAN_INSTALL_LAYOUT_PASS" } else { Write-Host ("CLEAN_INSTALL_LAYOUT_FAIL ({0})" -f $failedLayout) }
exit $(if ($failedLayout -eq 0) { 0 } else { 1 })
