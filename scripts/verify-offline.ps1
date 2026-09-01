$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$outputDirectory = Join-Path $repoRoot "test-output\offline-verification"
$stdoutPath = Join-Path $outputDirectory "harness.stdout.txt"
$stderrPath = Join-Path $outputDirectory "harness.stderr.txt"
$reportPath = Join-Path $outputDirectory "offline-verification.md"
$externalConnections = @()
$observedConnections = @()

New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
Set-Location $repoRoot

Write-Host "`n===== RELEASE BUILD ====="
cargo build --release -p r2h-second-brain-desktop
if ($LASTEXITCODE -ne 0) { throw "release application build failed" }

Write-Host "`n===== RELEASE OFFLINE HARNESS BUILD ====="
cargo test --release -p knowledge-e2e --test security_untrusted_content --no-run
if ($LASTEXITCODE -ne 0) { throw "release offline harness build failed" }

$harness = Get-ChildItem -Path (Join-Path $repoRoot "target\release\deps") -Filter "security_untrusted_content-*.exe" |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
if ($null -eq $harness) { throw "release offline harness executable was not found" }
if (-not (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue)) {
    throw "Get-NetTCPConnection is required to verify runtime network isolation"
}

Remove-Item -Force -ErrorAction SilentlyContinue $stdoutPath, $stderrPath
$env:R2H_OFFLINE_OBSERVE_MS = "5000"

try {
    Write-Host "`n===== OFFLINE RUNTIME OBSERVATION ====="
    $process = Start-Process `
        -FilePath $harness.FullName `
        -ArgumentList "--nocapture" `
        -PassThru `
        -WindowStyle Hidden `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath
    $processHandle = $process.Handle

    while (-not $process.HasExited) {
        $connections = @(Get-NetTCPConnection -OwningProcess $process.Id -ErrorAction SilentlyContinue)
        foreach ($connection in $connections) {
            $record = [PSCustomObject]@{
                State = [string]$connection.State
                LocalAddress = [string]$connection.LocalAddress
                LocalPort = [int]$connection.LocalPort
                RemoteAddress = [string]$connection.RemoteAddress
                RemotePort = [int]$connection.RemotePort
            }
            $observedConnections += $record
            $remote = $record.RemoteAddress
            $isExternal = $remote -notin @("", "0.0.0.0", "::", "127.0.0.1", "::1") -and
                $record.State -notin @("Bound", "Closed", "Listen")
            if ($isExternal) { $externalConnections += $record }
        }
        Start-Sleep -Milliseconds 100
        $process.Refresh()
    }
    $process.WaitForExit()
    $process.Refresh()
    $exitCode = $process.ExitCode
}
finally {
    Remove-Item Env:R2H_OFFLINE_OBSERVE_MS -ErrorAction SilentlyContinue
}

$stdout = if (Test-Path $stdoutPath) { Get-Content -Raw $stdoutPath } else { "" }
$stderr = if (Test-Path $stderrPath) { Get-Content -Raw $stderrPath } else { "" }
$lifecyclePassed = $exitCode -eq 0 -and $stdout.Contains("OFFLINE_HARNESS_PASS")
$uniqueConnections = @($observedConnections |
    Sort-Object State, LocalAddress, LocalPort, RemoteAddress, RemotePort -Unique)

$connectionEvidence = if ($uniqueConnections.Count -eq 0) {
    "No TCP connections were owned by the harness process during observation."
}
else {
    $rows = $uniqueConnections | ForEach-Object {
        "| $($_.State) | $($_.LocalAddress):$($_.LocalPort) | $($_.RemoteAddress):$($_.RemotePort) |"
    }
    (@(
        "| State | Local | Remote |"
        "| --- | --- | --- |"
    ) + $rows) -join "`n"
}

$marker = if ($lifecyclePassed -and $externalConnections.Count -eq 0) {
    "KNOWLEDGE_CORE_OFFLINE_VERIFICATION_PASS"
}
else {
    "KNOWLEDGE_CORE_OFFLINE_VERIFICATION_FAIL"
}
$timestamp = [DateTime]::UtcNow.ToString("o")
$report = @"
# Knowledge Core Offline Verification

- UTC: $timestamp
- Release application build: PASS
- Release harness: $($harness.FullName)
- Harness lifecycle (ingest, search, citation, audit): $(if ($lifecyclePassed) { "PASS" } else { "FAIL" })
- Harness exit code: $exitCode
- External TCP connections: $($externalConnections.Count)

## Observed TCP connections

$connectionEvidence

## Harness stdout

~~~text
$stdout
~~~

## Harness stderr

~~~text
$stderr
~~~

$marker
"@
Set-Content -Path $reportPath -Value $report -Encoding utf8

if (-not $lifecyclePassed) { throw "offline lifecycle harness failed; see $reportPath" }
if ($externalConnections.Count -ne 0) { throw "external TCP connection detected; see $reportPath" }

Write-Host "Evidence: $reportPath"
Write-Host "`nKNOWLEDGE_CORE_OFFLINE_VERIFICATION_PASS"
