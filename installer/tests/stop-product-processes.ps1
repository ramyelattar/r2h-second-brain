<#
.SYNOPSIS
  Path-verified stop of every process owned by the R2H product or the
  development tree. Used before clean-install tests so no dev-tree runtime
  can satisfy the installed product's service checks.
#>
param(
    [switch]$IncludeDevelopment
)

$ErrorActionPreference = "Continue"

function Stop-UnderPath {
    param([string[]]$Roots, [string[]]$ImageNames)
    $rootsLower = $Roots | ForEach-Object { $_.ToLower() }
    $procs = Get-CimInstance Win32_Process | Where-Object {
        $ep = $_.ExecutablePath
        if (-not $ep) { return $false }
        $epLower = $ep.ToLower()
        $matched = $false
        foreach ($r in $rootsLower) {
            if ($epLower.StartsWith($r)) { $matched = $true; break }
        }
        if (-not $matched -and $ImageNames) {
            $name = Split-Path $ep -Leaf
            if ($ImageNames -contains $name) { $matched = $true }
        }
        $matched
    }
    foreach ($p in $procs) {
        Write-Host ("Stopping pid={0} path={1}" -f $p.ProcessId, $p.ExecutablePath)
        taskkill /PID $p.ProcessId /T /F 2>$null | Out-Null
    }
    if (-not $procs) { Write-Host "No matching processes." }
}

$packRoot = "C:\ProgramData\R2H.AI-ELE"
$appHome = "C:\ProgramData\R2H\r2h-second-brain"
$appDir = "C:\Program Files\R2H\r2h-second-brain"

$roots = @($packRoot + "\local-ai\runtimes", $packRoot + "\local-ai\python-runtime", $appHome + "\engines\khoj", $appDir)
$images = @("r2h-second-brain-app.exe", "postgres.exe")
if ($IncludeDevelopment) {
    $roots += @(
        "E:\Projects\R2H-Desktop\01-R2H-PRODUCTS\r2h-second-brain\target",
        "E:\Projects\R2H-Desktop\01-R2H-PRODUCTS\r2h-second-brain\engines",
        "E:\Projects\R2H-Desktop\01-R2H-PRODUCTS\r2h-second-brain\run-data"
    )
    $images += @("llama-server.exe", "python.exe")
}

Stop-UnderPath -Roots $roots -ImageNames $images
Start-Sleep -Seconds 3
Write-Host "---remaining listeners on 42110-42113---"
Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
    Where-Object { $_.LocalPort -in 42110, 42111, 42112, 42113 } |
    ForEach-Object {
        $proc = Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue
        "{0} pid={1} path={2}" -f $_.LocalPort, $_.OwningProcess, $proc.Path
    }
