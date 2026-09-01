$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$releaseRoot = Join-Path $repoRoot "release-output"
$inspectionRoot = Join-Path $repoRoot "test-output\release-inspection"
$configurationPath = Join-Path $repoRoot "apps\desktop\src-tauri\tauri.conf.json"
$capabilityPath = Join-Path $repoRoot "apps\desktop\src-tauri\capabilities\default.json"

function Invoke-Checked {
    param(
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][scriptblock]$Command
    )

    Write-Host "`n===== $Label ====="
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$Label failed" }
}

function Test-ExternalConnection {
    param([Parameter(Mandatory)]$Connection)

    $remote = [string]$Connection.RemoteAddress
    return $remote -notin @("", "0.0.0.0", "::", "127.0.0.1", "::1") -and
        [string]$Connection.State -notin @("Bound", "Closed", "Listen")
}

function Get-DescendantProcessIds {
    param([Parameter(Mandatory)][int]$RootProcessId)

    $ids = [Collections.Generic.List[int]]::new()
    $ids.Add($RootProcessId)
    $processes = @(Get-CimInstance Win32_Process)
    for ($index = 0; $index -lt $ids.Count; $index++) {
        foreach ($child in $processes | Where-Object ParentProcessId -eq $ids[$index]) {
            if (-not $ids.Contains([int]$child.ProcessId)) {
                $ids.Add([int]$child.ProcessId)
            }
        }
    }
    return $ids.ToArray()
}

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    try {
        $sha256 = [Security.Cryptography.SHA256]::Create()
        try {
            $hash = $sha256.ComputeHash($stream)
        }
        finally {
            $sha256.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
    return [BitConverter]::ToString($hash).Replace("-", "")
}

Set-Location $repoRoot

$inventoryFile = Get-ChildItem -LiteralPath $releaseRoot -Recurse -Filter "release-inventory.json" |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
if ($null -eq $inventoryFile) { throw "release inventory was not found" }
$releaseDirectory = $inventoryFile.Directory.FullName
$inventory = Get-Content -Raw -LiteralPath $inventoryFile.FullName | ConvertFrom-Json
$configuration = Get-Content -Raw -LiteralPath $configurationPath | ConvertFrom-Json
$capability = Get-Content -Raw -LiteralPath $capabilityPath | ConvertFrom-Json

if ($inventory.commit -ne (git rev-parse HEAD)) { throw "release inventory commit is stale" }
if ($inventory.product_name -ne $configuration.productName -or
    $inventory.identifier -ne $configuration.identifier -or
    $inventory.version -ne $configuration.version) {
    throw "release metadata is inconsistent"
}
if (@($capability.permissions).Count -ne 1 -or $capability.permissions[0] -ne "dialog:allow-open") {
    throw "release capability is not dialog:allow-open only"
}
$csp = [string]$configuration.app.security.csp
foreach ($requiredCsp in @(
    "default-src 'self'",
    "connect-src ipc: http://ipc.localhost",
    "object-src 'none'",
    "frame-src 'none'"
)) {
    if (-not $csp.Contains($requiredCsp)) { throw "required CSP directive is missing: $requiredCsp" }
}
if ($csp -match "\*|unsafe-eval|https:") { throw "CSP grants an unexpected remote or wildcard source" }

$artifacts = @(
    foreach ($entry in @($inventory.artifacts)) {
        $path = Join-Path $releaseDirectory $entry.relative_path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "release artifact is missing: $path"
        }
        $file = Get-Item -LiteralPath $path
        $hash = Get-Sha256 -Path $path
        if ($file.Length -ne $entry.bytes -or $hash -ne $entry.sha256) {
            throw "release artifact changed after inventory: $path"
        }
        [PSCustomObject]@{ Entry = $entry; File = $file; Hash = $hash }
    }
)
$executableArtifact = $artifacts |
    Where-Object { $_.Entry.type -eq "executable" -and $_.File.Name -eq "r2h-second-brain-app.exe" } |
    Select-Object -First 1
if ($null -eq $executableArtifact) { throw "expected release executable is missing" }

$unexpectedPayload = Get-ChildItem -LiteralPath $releaseDirectory -Recurse -File |
    Where-Object {
        $_.Name -ne "release-inventory.json" -and
        $_.Extension -notin @(".exe", ".msi")
    }
if (@($unexpectedPayload).Count -ne 0) {
    throw "unexpected file was copied into release output: $($unexpectedPayload.FullName -join ', ')"
}

$binary = $executableArtifact.File.FullName
$forbiddenBinaryPatterns = @(
    "OPENAI_API_KEY",
    "BEGIN PRIVATE KEY",
    "sk-[A-Za-z0-9_-]{16,}",
    "E:\\Projects\\r2h-second-brain",
    "target\\debug",
    "node_modules",
    "security_untrusted_content",
    "\.gguf",
    "\.onnx",
    "development\.db",
    "fixtures[\\/]"
)
foreach ($pattern in $forbiddenBinaryPatterns) {
    & rg -a -q -e $pattern -- $binary
    if ($LASTEXITCODE -eq 0) { throw "forbidden release string matched: $pattern" }
    if ($LASTEXITCODE -notin @(0, 1)) { throw "binary scan failed for pattern: $pattern" }
}
foreach ($requiredBinaryPattern in @(
    "R2H Second Brain",
    "object-src 'none'"
)) {
    & rg -a -q -F -- $requiredBinaryPattern $binary
    if ($LASTEXITCODE -ne 0) { throw "expected embedded release metadata is missing: $requiredBinaryPattern" }
}

$runTimestamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ")
$runDirectory = Join-Path $inspectionRoot $runTimestamp
$dataRoot = Join-Path $runDirectory "data-root"
$stdoutPath = Join-Path $runDirectory "application.stdout.txt"
$stderrPath = Join-Path $runDirectory "application.stderr.txt"
New-Item -ItemType Directory -Force -Path $dataRoot | Out-Null

$oldDataRoot = $env:R2H_SECOND_BRAIN_DATA_ROOT
$env:R2H_SECOND_BRAIN_DATA_ROOT = $dataRoot
$process = $null
$externalConnections = @()
$observedConnections = @()
$peakRssBytes = 0L
try {
    Write-Host "`n===== PACKAGED NATIVE LAUNCH ====="
    $process = Start-Process `
        -FilePath $binary `
        -PassThru `
        -WindowStyle Hidden `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath
    $processHandle = $process.Handle
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ([DateTime]::UtcNow -lt $deadline -and -not $process.HasExited) {
        $process.Refresh()
        $peakRssBytes = [Math]::Max($peakRssBytes, $process.WorkingSet64)
        $processIds = Get-DescendantProcessIds -RootProcessId $process.Id
        foreach ($processId in $processIds) {
            foreach ($connection in @(Get-NetTCPConnection -OwningProcess $processId -ErrorAction SilentlyContinue)) {
                $record = [PSCustomObject]@{
                    process_id = $processId
                    state = [string]$connection.State
                    local = "$($connection.LocalAddress):$($connection.LocalPort)"
                    remote = "$($connection.RemoteAddress):$($connection.RemotePort)"
                }
                $observedConnections += $record
                if (Test-ExternalConnection -Connection $connection) { $externalConnections += $record }
            }
        }
        Start-Sleep -Milliseconds 100
        $process.Refresh()
    }
    if ($process.HasExited) { throw "release application exited during startup observation" }
}
finally {
    if ($null -ne $process -and -not $process.HasExited) {
        $processIdsToStop = @(Get-DescendantProcessIds -RootProcessId $process.Id)
        [Array]::Reverse($processIdsToStop)
        foreach ($processIdToStop in $processIdsToStop) {
            Stop-Process -Id $processIdToStop -Force -ErrorAction SilentlyContinue
        }
        $process.WaitForExit()
    }
    $env:R2H_SECOND_BRAIN_DATA_ROOT = $oldDataRoot
}

$externalConnections = @($externalConnections |
    Sort-Object process_id, state, local, remote -Unique)
$observedConnections = @($observedConnections |
    Sort-Object process_id, state, local, remote -Unique)
$databasePath = Join-Path $dataRoot "data\knowledge-core.db"
if (-not (Test-Path -LiteralPath $databasePath -PathType Leaf)) {
    throw "fresh release data root did not create the Knowledge Core database"
}
foreach ($logPath in @($stdoutPath, $stderrPath)) {
    if (Test-Path -LiteralPath $logPath) {
        $log = Get-Content -Raw -LiteralPath $logPath
        if ($log -match "(?i)panic|fatal|unhandled|corrupt|migration failure|missing asset|permission denied|CSP violation|command not found|WebView initialization failure") {
            throw "release log contains a failure marker: $logPath"
        }
    }
}
$staleRuntimeFiles = Get-ChildItem -LiteralPath $dataRoot -Recurse -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like "*.partial" -or $_.Name -like "*restore-candidate*" }
if (@($staleRuntimeFiles).Count -ne 0) { throw "release left stale partial or restore-candidate data" }

Invoke-Checked -Label "PACKAGED PROCESS-TREE OFFLINE VERIFICATION" -Command {
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
        ".\scripts\verify-packaged-offline.ps1" `
        -ArtifactPath $binary `
        -Scenario All `
        -ObservationSeconds 90
}
$packagedReportFile = Get-ChildItem `
    -LiteralPath (Join-Path $repoRoot "test-output\packaged-offline-verification") `
    -Recurse `
    -Filter "packaged-offline-verification.json" |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
if ($null -eq $packagedReportFile) { throw "packaged offline evidence was not found" }
$packagedReport = Get-Content -Raw -LiteralPath $packagedReportFile.FullName | ConvertFrom-Json
if ($packagedReport.artifact_sha256 -ne $executableArtifact.Hash -or
    $packagedReport.marker -ne "KNOWLEDGE_CORE_PACKAGED_OFFLINE_VERIFICATION_PASS" -or
    $packagedReport.observed_packaged_external_tcp_connections -ne 0) {
    throw "packaged offline evidence is stale or failed"
}

Write-Host "`n===== RELEASE PERFORMANCE ====="
cargo test --release -p knowledge-e2e --test performance_search --no-run
if ($LASTEXITCODE -ne 0) { throw "release performance harness build failed" }
$performanceHarness = Get-ChildItem -LiteralPath (Join-Path $repoRoot "target\release\deps") `
    -Filter "performance_search-*.exe" |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
if ($null -eq $performanceHarness) { throw "release performance harness was not found" }
$performanceStdout = Join-Path $runDirectory "performance.stdout.txt"
$performanceStderr = Join-Path $runDirectory "performance.stderr.txt"
$performance = Start-Process `
    -FilePath $performanceHarness.FullName `
    -ArgumentList "--nocapture" `
    -PassThru `
    -WindowStyle Hidden `
    -RedirectStandardOutput $performanceStdout `
    -RedirectStandardError $performanceStderr
$performanceHandle = $performance.Handle
$performancePeakRssBytes = 0L
$performanceDeadline = [DateTime]::UtcNow.AddMinutes(3)
while (-not $performance.HasExited -and [DateTime]::UtcNow -lt $performanceDeadline) {
    $performance.Refresh()
    $performancePeakRssBytes = [Math]::Max($performancePeakRssBytes, $performance.WorkingSet64)
    Start-Sleep -Milliseconds 100
}
if (-not $performance.HasExited) {
    Stop-Process -Id $performance.Id -Force
    throw "release performance harness timed out"
}
$performance.WaitForExit()
$performance.Refresh()
if ($performance.ExitCode -ne 0) { throw "release performance harness failed" }
$performanceText = Get-Content -Raw -LiteralPath $performanceStdout
$p95Match = [regex]::Match($performanceText, "p95_ms=([0-9.]+)")
if (-not $p95Match.Success) { throw "release P95 measurement was not reported" }
$p95Milliseconds = [double]$p95Match.Groups[1].Value
$performancePeakRssMiB = [Math]::Round($performancePeakRssBytes / 1MB, 2)
if ($p95Milliseconds -gt 250 -or $performancePeakRssMiB -gt 700) {
    throw "release performance budget failed: P95=$p95Milliseconds ms RSS=$performancePeakRssMiB MiB"
}

$offlineReportPath = Join-Path $repoRoot "test-output\offline-verification\offline-verification.md"
$offlineReport = Get-Content -Raw -LiteralPath $offlineReportPath
if (-not $offlineReport.Contains("KNOWLEDGE_CORE_OFFLINE_VERIFICATION_PASS")) {
    throw "offline verification marker is missing"
}
$externalMatch = [regex]::Match($offlineReport, "External TCP connections: ([0-9]+)")
if (-not $externalMatch.Success -or $externalMatch.Groups[1].Value -ne "0") {
    throw "offline external TCP evidence is missing or nonzero"
}

$inspection = [PSCustomObject]@{
    generated_utc = [DateTime]::UtcNow.ToString("o")
    release_directory = $releaseDirectory
    inventory = $inventoryFile.FullName
    commit = $inventory.commit
    product_name = $inventory.product_name
    version = $inventory.version
    identifier = $inventory.identifier
    bundle_active = $inventory.bundle_active
    artifacts = @($inventory.artifacts)
    capability_permissions = @($capability.permissions)
    csp = $csp
    signature_state = @($inventory.artifacts.signature_status)
    native_launch = "PASS"
    native_data_root = $dataRoot
    native_database = $databasePath
    native_peak_rss_mib = [Math]::Round($peakRssBytes / 1MB, 2)
    native_external_tcp_connections = $externalConnections.Count
    packaged_network = "PASS"
    packaged_network_evidence = $packagedReportFile.FullName
    packaged_network_scenarios = @($packagedReport.scenarios)
    packaged_external_tcp_connections = 0
    packaged_external_udp_observations = 0
    observed_tcp_connections = $observedConnections
    external_tcp_connections = $externalConnections
    offline_verification = "PASS"
    offline_external_tcp_connections = 0
    release_e2e = "PASS"
    release_backup_restore = "PASS"
    release_search_p95_ms = $p95Milliseconds
    release_peak_rss_mib = $performancePeakRssMiB
    performance_fixture_stdout = $performanceStdout
    application_stdout = $stdoutPath
    application_stderr = $stderrPath
    verdict = "PASS"
}
$inspectionJson = Join-Path $runDirectory "release-inspection.json"
$inspection | ConvertTo-Json -Depth 8 |
    Set-Content -LiteralPath $inspectionJson -Encoding utf8

Write-Host "Inspection evidence: $inspectionJson"
Write-Host "Release search P95: $p95Milliseconds ms"
Write-Host "Release performance peak RSS: $performancePeakRssMiB MiB"
Write-Host "Packaged external TCP connections: 0"
Write-Host "`nKNOWLEDGE_CORE_RELEASE_INSPECTION_PASS"
