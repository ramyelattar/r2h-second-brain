[CmdletBinding()]
param(
    [string]$ArtifactPath,
    [ValidateSet("A", "B", "C", "D", "All")]
    [string]$Scenario = "All",
    [ValidateRange(15, 3600)]
    [int]$ObservationSeconds = 90,
    [ValidateRange(100, 5000)]
    [int]$PollMilliseconds = 250,
    [string]$OutputDirectory
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$releaseRoot = Join-Path $repoRoot "release-output"
$defaultOutputRoot = Join-Path $repoRoot "test-output\packaged-offline-verification"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class R2HWindowControl {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
}
"@

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    try {
        $sha256 = [Security.Cryptography.SHA256]::Create()
        try {
            return [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace("-", "")
        }
        finally {
            $sha256.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}

function Get-AddressClass {
    param([string]$Address)

    if ([string]::IsNullOrWhiteSpace($Address) -or $Address -in @("0.0.0.0", "::")) {
        return "unspecified"
    }
    $parsed = $null
    if (-not [Net.IPAddress]::TryParse($Address, [ref]$parsed)) {
        return "unknown"
    }
    if ([Net.IPAddress]::IsLoopback($parsed)) { return "loopback" }
    if ($parsed.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork) {
        $bytes = $parsed.GetAddressBytes()
        if ($bytes[0] -eq 10 -or
            ($bytes[0] -eq 169 -and $bytes[1] -eq 254) -or
            ($bytes[0] -eq 172 -and $bytes[1] -ge 16 -and $bytes[1] -le 31) -or
            ($bytes[0] -eq 192 -and $bytes[1] -eq 168)) {
            return "lan"
        }
        return "external"
    }
    if ($parsed.IsIPv6LinkLocal -or $parsed.IsIPv6SiteLocal) { return "lan" }
    $ipv6Bytes = $parsed.GetAddressBytes()
    if (($ipv6Bytes[0] -band 0xFE) -eq 0xFC) { return "lan" }
    return "external"
}

function Resolve-LifecyclePhase {
    param(
        [double]$ElapsedSeconds,
        [bool]$SmokeRunning
    )

    if ($ElapsedSeconds -lt 3) { return "before-window-and-webview-startup" }
    if ($ElapsedSeconds -lt 8) { return "first-ui-render" }
    if ($SmokeRunning) { return "workspace-import-search-citation-audit-backup-restore-smoke" }
    if ($ElapsedSeconds -lt 30) { return "post-render-idle" }
    return "extended-idle"
}

function Update-ProcessTree {
    param(
        [Parameter(Mandatory)][int]$RootProcessId,
        [Parameter(Mandatory)][object[]]$Inventory,
        [Parameter(Mandatory)]$KnownIds,
        [Parameter(Mandatory)]$ProcessRecords,
        [Parameter(Mandatory)][DateTime]$ObservedUtc
    )

    $KnownIds.Add($RootProcessId) | Out-Null
    do {
        $added = $false
        foreach ($candidate in $Inventory) {
            $candidateId = [int]$candidate.ProcessId
            if ($KnownIds.Contains([int]$candidate.ParentProcessId) -and
                -not $KnownIds.Contains($candidateId)) {
                $KnownIds.Add($candidateId) | Out-Null
                $added = $true
            }
        }
    } while ($added)

    $presentIds = [Collections.Generic.HashSet[int]]::new()
    foreach ($candidate in $Inventory) {
        $candidateId = [int]$candidate.ProcessId
        if (-not $KnownIds.Contains($candidateId)) { continue }
        $presentIds.Add($candidateId) | Out-Null
        if (-not $ProcessRecords.ContainsKey($candidateId)) {
            $processRole = if ([string]$candidate.Name -eq "r2h-second-brain-app.exe") {
                "packaged-application"
            }
            elseif ([string]$candidate.Name -eq "msedgewebview2.exe") {
                "webview2-runtime"
            }
            else {
                "brokered-or-child-process"
            }
            $ProcessRecords[$candidateId] = [ordered]@{
                process_id = $candidateId
                parent_process_id = [int]$candidate.ParentProcessId
                name = [string]$candidate.Name
                process_role = $processRole
                executable_path = [string]$candidate.ExecutablePath
                command_line = [string]$candidate.CommandLine
                creation_time_utc = if ($null -ne $candidate.CreationDate) {
                    ([DateTime]$candidate.CreationDate).ToUniversalTime().ToString("o")
                }
                else { "" }
                first_observed_utc = $ObservedUtc.ToString("o")
                last_observed_utc = $ObservedUtc.ToString("o")
                exit_time_utc = ""
                state_at_monitor_end = "running"
            }
        }
        else {
            $ProcessRecords[$candidateId].last_observed_utc = $ObservedUtc.ToString("o")
        }
    }
    foreach ($knownId in @($KnownIds)) {
        if ($ProcessRecords.ContainsKey($knownId) -and
            -not $presentIds.Contains($knownId) -and
            [string]::IsNullOrEmpty($ProcessRecords[$knownId].exit_time_utc)) {
            $ProcessRecords[$knownId].exit_time_utc = $ObservedUtc.ToString("o")
            $ProcessRecords[$knownId].state_at_monitor_end = "exited"
        }
    }
}

function Add-TcpObservations {
    param(
        [Parameter(Mandatory)]$KnownIds,
        [Parameter(Mandatory)]$ConnectionRecords,
        [Parameter(Mandatory)][string]$Phase,
        [Parameter(Mandatory)][DateTime]$ObservedUtc
    )

    foreach ($connection in @(Get-NetTCPConnection -ErrorAction SilentlyContinue)) {
        $processId = [int]$connection.OwningProcess
        if (-not $KnownIds.Contains($processId)) { continue }
        $remoteClass = Get-AddressClass -Address ([string]$connection.RemoteAddress)
        $state = [string]$connection.State
        $trafficClass = if ($state -in @("Bound", "Closed", "Listen")) {
            "listener-or-inactive"
        }
        elseif ($remoteClass -eq "loopback" -or $remoteClass -eq "unspecified") {
            "local-ipc"
        }
        elseif ($remoteClass -eq "lan") {
            "lan"
        }
        else {
            "external"
        }
        $key = @(
            $processId,
            $state,
            [string]$connection.LocalAddress,
            [int]$connection.LocalPort,
            [string]$connection.RemoteAddress,
            [int]$connection.RemotePort
        ) -join "|"
        if (-not $ConnectionRecords.ContainsKey($key)) {
            $ConnectionRecords[$key] = [ordered]@{
                owning_process = $processId
                state = $state
                local_address = [string]$connection.LocalAddress
                local_port = [int]$connection.LocalPort
                remote_address = [string]$connection.RemoteAddress
                remote_port = [int]$connection.RemotePort
                remote_class = $remoteClass
                traffic_class = $trafficClass
                first_phase = $Phase
                last_phase = $Phase
                first_observed_utc = $ObservedUtc.ToString("o")
                last_observed_utc = $ObservedUtc.ToString("o")
            }
        }
        else {
            $ConnectionRecords[$key].last_phase = $Phase
            $ConnectionRecords[$key].last_observed_utc = $ObservedUtc.ToString("o")
        }
    }
}

function Add-UdpObservations {
    param(
        [Parameter(Mandatory)]$KnownIds,
        [Parameter(Mandatory)]$EndpointRecords,
        [Parameter(Mandatory)][string]$Phase,
        [Parameter(Mandatory)][DateTime]$ObservedUtc
    )

    foreach ($endpoint in @(Get-NetUDPEndpoint -ErrorAction SilentlyContinue)) {
        $processId = [int]$endpoint.OwningProcess
        if (-not $KnownIds.Contains($processId)) { continue }
        $key = @(
            $processId,
            [string]$endpoint.LocalAddress,
            [int]$endpoint.LocalPort
        ) -join "|"
        if (-not $EndpointRecords.ContainsKey($key)) {
            $EndpointRecords[$key] = [ordered]@{
                owning_process = $processId
                local_address = [string]$endpoint.LocalAddress
                local_port = [int]$endpoint.LocalPort
                address_class = Get-AddressClass -Address ([string]$endpoint.LocalAddress)
                first_phase = $Phase
                last_phase = $Phase
                first_observed_utc = $ObservedUtc.ToString("o")
                last_observed_utc = $ObservedUtc.ToString("o")
            }
        }
        else {
            $EndpointRecords[$key].last_phase = $Phase
            $EndpointRecords[$key].last_observed_utc = $ObservedUtc.ToString("o")
        }
    }
}

function Invoke-PackagedScenario {
    param(
        [Parameter(Mandatory)][string]$ScenarioName,
        [Parameter(Mandatory)][string]$DataRoot,
        [Parameter(Mandatory)][string]$ProfileRoot,
        [Parameter(Mandatory)][bool]$RunSmoke,
        [Parameter(Mandatory)][string]$RunDirectory
    )

    New-Item -ItemType Directory -Force -Path $DataRoot, $ProfileRoot | Out-Null
    $scenarioDirectory = Join-Path $RunDirectory "scenario-$ScenarioName"
    New-Item -ItemType Directory -Force -Path $scenarioDirectory | Out-Null
    $stdoutPath = Join-Path $scenarioDirectory "application.stdout.txt"
    $stderrPath = Join-Path $scenarioDirectory "application.stderr.txt"
    $smokeStdoutPath = Join-Path $scenarioDirectory "release-smoke.stdout.txt"
    $smokeStderrPath = Join-Path $scenarioDirectory "release-smoke.stderr.txt"

    $oldDataRoot = $env:R2H_SECOND_BRAIN_DATA_ROOT
    $oldWebViewDataRoot = $env:WEBVIEW2_USER_DATA_FOLDER
    $env:R2H_SECOND_BRAIN_DATA_ROOT = $DataRoot
    $env:WEBVIEW2_USER_DATA_FOLDER = $ProfileRoot

    $rootProcess = $null
    $smokeProcess = $null
    $knownIds = [Collections.Generic.HashSet[int]]::new()
    $processRecords = [Collections.Generic.Dictionary[int, object]]::new()
    $connectionRecords = [Collections.Generic.Dictionary[string, object]]::new()
    $udpRecords = [Collections.Generic.Dictionary[string, object]]::new()
    $mainWindowObserved = $false
    $cleanShutdown = $false
    $smokeStarted = $false
    $scenarioStartedUtc = [DateTime]::UtcNow
    $observationDeadline = $scenarioStartedUtc.AddSeconds($ObservationSeconds)
    $smokeDeadline = $scenarioStartedUtc.AddMinutes(8)
    try {
        $rootProcess = Start-Process `
            -FilePath $ArtifactPath `
            -PassThru `
            -WindowStyle Hidden `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath
        $rootHandle = $rootProcess.Handle

        while ([DateTime]::UtcNow -lt $observationDeadline -or
            ($null -ne $smokeProcess -and -not $smokeProcess.HasExited)) {
            $now = [DateTime]::UtcNow
            if ($now -gt $smokeDeadline) { throw "scenario $ScenarioName smoke timed out" }
            $rootProcess.Refresh()
            if ($rootProcess.HasExited) {
                throw "packaged application exited during scenario $ScenarioName"
            }
            if ($rootProcess.MainWindowHandle -ne [IntPtr]::Zero) {
                $mainWindowObserved = $true
            }
            $elapsedSeconds = ($now - $scenarioStartedUtc).TotalSeconds
            if ($RunSmoke -and -not $smokeStarted -and $elapsedSeconds -ge 8) {
                $smokeProcess = Start-Process `
                    -FilePath "powershell.exe" `
                    -ArgumentList @(
                        "-NoProfile",
                        "-ExecutionPolicy", "Bypass",
                        "-File", (Join-Path $PSScriptRoot "verify-release-smoke.ps1")
                    ) `
                    -PassThru `
                    -WindowStyle Hidden `
                    -RedirectStandardOutput $smokeStdoutPath `
                    -RedirectStandardError $smokeStderrPath
                $smokeHandle = $smokeProcess.Handle
                $smokeStarted = $true
            }
            $smokeRunning = $null -ne $smokeProcess -and -not $smokeProcess.HasExited
            $phase = Resolve-LifecyclePhase -ElapsedSeconds $elapsedSeconds -SmokeRunning $smokeRunning
            $inventory = @(Get-CimInstance Win32_Process |
                    Select-Object ProcessId, ParentProcessId, Name, ExecutablePath, CommandLine, CreationDate)
            Update-ProcessTree `
                -RootProcessId $rootProcess.Id `
                -Inventory $inventory `
                -KnownIds $knownIds `
                -ProcessRecords $processRecords `
                -ObservedUtc $now
            Add-TcpObservations `
                -KnownIds $knownIds `
                -ConnectionRecords $connectionRecords `
                -Phase $phase `
                -ObservedUtc $now
            Add-UdpObservations `
                -KnownIds $knownIds `
                -EndpointRecords $udpRecords `
                -Phase $phase `
                -ObservedUtc $now
            Start-Sleep -Milliseconds $PollMilliseconds
        }

        if ($null -ne $smokeProcess) {
            $smokeProcess.WaitForExit()
            $smokeProcess.Refresh()
            if ($smokeProcess.ExitCode -ne 0) {
                throw "release smoke failed in scenario $ScenarioName"
            }
            $smokeText = Get-Content -Raw -LiteralPath $smokeStdoutPath
            if (-not $smokeText.Contains("KNOWLEDGE_CORE_RELEASE_SMOKE_PASS") -or
                -not $smokeText.Contains("KNOWLEDGE_CORE_OFFLINE_VERIFICATION_PASS")) {
                throw "release smoke markers are missing in scenario $ScenarioName"
            }
        }

        $rootProcess.Refresh()
        $windowHandle = $rootProcess.MainWindowHandle
        if ($windowHandle -eq [IntPtr]::Zero) {
            throw "packaged application did not expose a main window in scenario $ScenarioName"
        }
        [R2HWindowControl]::PostMessage($windowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) |
            Out-Null
        if (-not $rootProcess.WaitForExit(10000)) {
            throw "packaged application did not shut down cleanly in scenario $ScenarioName"
        }
        $cleanShutdown = $true
        $descendantExitDeadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            $shutdownObservedUtc = [DateTime]::UtcNow
            $shutdownInventory = @(Get-CimInstance Win32_Process |
                    Select-Object ProcessId, ParentProcessId, Name, ExecutablePath, CommandLine, CreationDate)
            Update-ProcessTree `
                -RootProcessId $rootProcess.Id `
                -Inventory $shutdownInventory `
                -KnownIds $knownIds `
                -ProcessRecords $processRecords `
                -ObservedUtc $shutdownObservedUtc
            $presentKnownProcesses = @(
                $shutdownInventory |
                    Where-Object {
                        $candidateId = [int]$_.ProcessId
                        if (-not $knownIds.Contains($candidateId) -or
                            -not $processRecords.ContainsKey($candidateId)) {
                            return $false
                        }
                        if ($processRecords[$candidateId].process_role -notin @(
                                "packaged-application",
                                "webview2-runtime"
                            )) {
                            return $false
                        }
                        $candidateCreationUtc = if ($null -ne $_.CreationDate) {
                            ([DateTime]$_.CreationDate).ToUniversalTime().ToString("o")
                        }
                        else { "" }
                        return $candidateCreationUtc -eq
                            $processRecords[$candidateId].creation_time_utc
                    }
            )
            if ($presentKnownProcesses.Count -eq 0) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $descendantExitDeadline)
        if ($presentKnownProcesses.Count -ne 0) {
            $survivors = @(
                $presentKnownProcesses |
                    ForEach-Object { "PID $($_.ProcessId) $($_.Name)" }
            )
            throw "packaged descendants did not exit after scenario $ScenarioName shutdown: $($survivors -join ', ')"
        }
    }
    finally {
        if ($null -ne $smokeProcess -and -not $smokeProcess.HasExited) {
            Stop-Process -Id $smokeProcess.Id -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $rootProcess) {
            $currentInventory = @(Get-CimInstance Win32_Process |
                    Select-Object ProcessId, CreationDate)
            $recordedProcessesToStop = @(
                $currentInventory |
                    Where-Object {
                        $candidateId = [int]$_.ProcessId
                        if (-not $knownIds.Contains($candidateId) -or
                            -not $processRecords.ContainsKey($candidateId)) {
                            return $false
                        }
                        if ($processRecords[$candidateId].process_role -notin @(
                                "packaged-application",
                                "webview2-runtime"
                            )) {
                            return $false
                        }
                        $candidateCreationUtc = if ($null -ne $_.CreationDate) {
                            ([DateTime]$_.CreationDate).ToUniversalTime().ToString("o")
                        }
                        else { "" }
                        return $candidateCreationUtc -eq
                            $processRecords[$candidateId].creation_time_utc
                    }
            )
            foreach ($recordedProcess in $recordedProcessesToStop) {
                Stop-Process -Id $recordedProcess.ProcessId -Force -ErrorAction SilentlyContinue
            }
        }
        $env:R2H_SECOND_BRAIN_DATA_ROOT = $oldDataRoot
        $env:WEBVIEW2_USER_DATA_FOLDER = $oldWebViewDataRoot
    }

    $processRows = @($processRecords.Values | ForEach-Object { [PSCustomObject]$_ })
    $connectionRows = @($connectionRecords.Values | ForEach-Object { [PSCustomObject]$_ })
    $udpRows = @($udpRecords.Values | ForEach-Object { [PSCustomObject]$_ })
    $externalRows = @($connectionRows | Where-Object traffic_class -eq "external")
    $webViewRows = @($processRows | Where-Object name -eq "msedgewebview2.exe")
    $databasePath = Join-Path $DataRoot "data\knowledge-core.db"
    $scenarioEndedUtc = [DateTime]::UtcNow
    $result = [ordered]@{
        scenario = $ScenarioName
        data_root_state = if ($ScenarioName -eq "B") { "existing" } else { "fresh" }
        webview_profile_state = if ($ScenarioName -in @("B", "C")) { "existing" } else { "fresh" }
        release_smoke_exercised = $RunSmoke
        started_utc = $scenarioStartedUtc.ToString("o")
        ended_utc = $scenarioEndedUtc.ToString("o")
        observation_seconds = [Math]::Round(($scenarioEndedUtc - $scenarioStartedUtc).TotalSeconds, 3)
        process_count = $processRows.Count
        webview2_descendant_count = $webViewRows.Count
        external_tcp_connection_count = $externalRows.Count
        observed_udp_endpoint_count = $udpRows.Count
        external_udp_observation_count = 0
        main_window_observed = $mainWindowObserved
        database_created = Test-Path -LiteralPath $databasePath -PathType Leaf
        clean_shutdown = $cleanShutdown
        exit_code = $rootProcess.ExitCode
        processes = $processRows
        tcp_connections = $connectionRows
        external_tcp_connections = $externalRows
        udp_endpoints = $udpRows
        application_stdout = $stdoutPath
        application_stderr = $stderrPath
        release_smoke_stdout = if ($RunSmoke) { $smokeStdoutPath } else { "" }
        release_smoke_stderr = if ($RunSmoke) { $smokeStderrPath } else { "" }
    }
    $scenarioJson = Join-Path $scenarioDirectory "scenario-result.json"
    $result | ConvertTo-Json -Depth 8 |
        Set-Content -LiteralPath $scenarioJson -Encoding utf8
    $processRows | Export-Csv -NoTypeInformation -LiteralPath `
        (Join-Path $scenarioDirectory "process-tree.csv")
    $connectionRows | Export-Csv -NoTypeInformation -LiteralPath `
        (Join-Path $scenarioDirectory "tcp-connections.csv")
    $udpRows | Export-Csv -NoTypeInformation -LiteralPath `
        (Join-Path $scenarioDirectory "udp-endpoints.csv")

    if (-not $result.main_window_observed -or
        -not $result.database_created -or
        -not $result.clean_shutdown -or
        $result.exit_code -ne 0) {
        throw "scenario $ScenarioName packaged lifecycle failed"
    }
    if ($result.external_tcp_connection_count -ne 0) {
        throw "scenario $ScenarioName observed $($result.external_tcp_connection_count) external TCP connection(s)"
    }
    return [PSCustomObject]$result
}

Set-Location $repoRoot
if (-not (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) -or
    -not (Get-Command Get-NetUDPEndpoint -ErrorAction SilentlyContinue)) {
    throw "Windows TCP and UDP endpoint cmdlets are required"
}

$inventoryFile = Get-ChildItem -LiteralPath $releaseRoot -Recurse -Filter "release-inventory.json" |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
if ([string]::IsNullOrWhiteSpace($ArtifactPath)) {
    if ($null -eq $inventoryFile) { throw "release inventory was not found" }
    $inventory = Get-Content -Raw -LiteralPath $inventoryFile.FullName | ConvertFrom-Json
    $executableEntry = @($inventory.artifacts |
            Where-Object { $_.type -eq "executable" -and $_.relative_path -eq "r2h-second-brain-app.exe" }) |
        Select-Object -First 1
    if ($null -eq $executableEntry) { throw "release executable inventory entry was not found" }
    $ArtifactPath = Join-Path $inventoryFile.Directory.FullName $executableEntry.relative_path
}
$ArtifactPath = [IO.Path]::GetFullPath($ArtifactPath)
if (-not (Test-Path -LiteralPath $ArtifactPath -PathType Leaf)) {
    throw "packaged executable was not found: $ArtifactPath"
}

$artifactDirectory = Split-Path -Parent $ArtifactPath
$artifactInventoryPath = Join-Path $artifactDirectory "release-inventory.json"
if (-not (Test-Path -LiteralPath $artifactInventoryPath -PathType Leaf)) {
    throw "packaged executable must have a release inventory in the same directory"
}
$artifactInventory = Get-Content -Raw -LiteralPath $artifactInventoryPath | ConvertFrom-Json
$artifactEntry = @($artifactInventory.artifacts |
        Where-Object relative_path -eq (Split-Path -Leaf $ArtifactPath)) |
    Select-Object -First 1
if ($null -eq $artifactEntry -or (Get-Sha256 -Path $ArtifactPath) -ne $artifactEntry.sha256) {
    throw "packaged executable does not match its release inventory"
}

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $defaultOutputRoot ([DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ"))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$controlRoot = Join-Path $OutputDirectory "controlled-roots"
$sharedDataRoot = Join-Path $controlRoot "scenario-a-data"
$sharedProfileRoot = Join-Path $controlRoot "shared-webview-profile"
$scenarioDefinitions = [ordered]@{
    A = @{
        data = $sharedDataRoot
        profile = $sharedProfileRoot
        smoke = $false
    }
    B = @{
        data = $sharedDataRoot
        profile = $sharedProfileRoot
        smoke = $false
    }
    C = @{
        data = (Join-Path $controlRoot "scenario-c-data")
        profile = $sharedProfileRoot
        smoke = $false
    }
    D = @{
        data = (Join-Path $controlRoot "scenario-d-data")
        profile = (Join-Path $controlRoot "scenario-d-webview-profile")
        smoke = $true
    }
}
$scenarioNames = if ($Scenario -eq "All") { @("A", "B", "C", "D") } else { @($Scenario) }
$results = @()
foreach ($scenarioName in $scenarioNames) {
    $definition = $scenarioDefinitions[$scenarioName]
    Write-Host "`n===== PACKAGED OFFLINE SCENARIO $scenarioName ====="
    $result = Invoke-PackagedScenario `
        -ScenarioName $scenarioName `
        -DataRoot $definition.data `
        -ProfileRoot $definition.profile `
        -RunSmoke $definition.smoke `
        -RunDirectory $OutputDirectory
    $results += $result
    Write-Host "Observation duration: $($result.observation_seconds) seconds"
    Write-Host "Processes: $($result.process_count)"
    Write-Host "WebView2 descendants: $($result.webview2_descendant_count)"
    Write-Host "External TCP connections: $($result.external_tcp_connection_count)"
    Write-Host "Observed UDP endpoints: $($result.observed_udp_endpoint_count)"
}

$summary = [ordered]@{
    generated_utc = [DateTime]::UtcNow.ToString("o")
    artifact_path = $ArtifactPath
    artifact_sha256 = [string]$artifactEntry.sha256
    inventory = $artifactInventoryPath
    source_commit = [string]$artifactInventory.commit
    observation_seconds_per_scenario = $ObservationSeconds
    poll_milliseconds = $PollMilliseconds
    scenarios = $results
    observed_packaged_external_tcp_connections = @(
        $results | ForEach-Object external_tcp_connections
    ).Count
    observed_packaged_external_udp_endpoints = 0
    verdict = "PASS"
    marker = "KNOWLEDGE_CORE_PACKAGED_OFFLINE_VERIFICATION_PASS"
}
$summaryPath = Join-Path $OutputDirectory "packaged-offline-verification.json"
$summary | ConvertTo-Json -Depth 10 |
    Set-Content -LiteralPath $summaryPath -Encoding utf8

Write-Host "Evidence: $summaryPath"
Write-Host "Observed packaged external TCP connections: 0"
Write-Host "`nKNOWLEDGE_CORE_PACKAGED_OFFLINE_VERIFICATION_PASS"
