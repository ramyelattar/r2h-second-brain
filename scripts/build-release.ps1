$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$releaseRoot = Join-Path $repoRoot "release-output"
$releaseExecutable = Join-Path $repoRoot "target\release\r2h-second-brain-app.exe"
$bundleRoot = Join-Path $repoRoot "target\release\bundle"
$allowedDirtyPaths = @(
    "2026-07-17-r2h-second-brain-knowledge-core-implementation-plan.md",
    "docs/release/KNOWLEDGE_CORE_PHASE_ACCEPTANCE.md"
)

function Remove-GeneratedPath {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$AllowedParent
    )

    $resolvedPath = [IO.Path]::GetFullPath($Path)
    $resolvedParent = [IO.Path]::GetFullPath($AllowedParent).TrimEnd("\") + "\"
    if (-not $resolvedPath.StartsWith($resolvedParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing to remove path outside generated output: $resolvedPath"
    }
    if (Test-Path -LiteralPath $resolvedPath) {
        Remove-Item -Recurse -Force -LiteralPath $resolvedPath
    }
}

function Get-SignatureState {
    param([Parameter(Mandatory)][string]$Path)

    $signTool = Get-ChildItem -LiteralPath "C:\Program Files (x86)\Windows Kits\10\bin" `
        -Recurse `
        -Filter "signtool.exe" `
        -ErrorAction SilentlyContinue |
        Where-Object FullName -Like "*\x64\signtool.exe" |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if ($null -eq $signTool) { throw "signtool.exe is required for release signature inspection" }

    $oldErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = (& $signTool.FullName verify /pa /all /v $Path 2>&1) -join "`n"
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $oldErrorActionPreference
    }
    if ($exitCode -eq 0) {
        $signer = [regex]::Match($output, "(?m)^\s*Issued to:\s*(.+)$")
        return [PSCustomObject]@{
            Status = "Valid"
            Signer = if ($signer.Success) { $signer.Groups[1].Value.Trim() } else { "" }
        }
    }
    if ($output.Contains("No signature found")) {
        return [PSCustomObject]@{ Status = "NotSigned"; Signer = "" }
    }
    throw "signature inspection failed: $output"
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

function Get-UnexpectedDirtyPaths {
    return @(
        git status --porcelain=v1 --untracked-files=all --no-renames |
            ForEach-Object {
                if ($_.Length -lt 4) { throw "unexpected git status output: $_" }
                $_.Substring(3).Trim('"').Replace("\", "/")
            } |
            Where-Object { $_ -notin $allowedDirtyPaths }
    )
}

Set-Location $repoRoot

$unexpectedDirtyPaths = @(Get-UnexpectedDirtyPaths)
if ($unexpectedDirtyPaths.Count -ne 0) {
    throw "release build requires committed source; unexpected paths: $($unexpectedDirtyPaths -join ', ')"
}

Remove-GeneratedPath -Path $releaseExecutable -AllowedParent (Join-Path $repoRoot "target\release")
Remove-GeneratedPath -Path $bundleRoot -AllowedParent (Join-Path $repoRoot "target\release")
New-Item -ItemType Directory -Force -Path $releaseRoot | Out-Null

Write-Host "`n===== PRE-RELEASE VERIFICATION ====="
powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "verify.ps1")
if ($LASTEXITCODE -ne 0) { throw "pre-release verification failed" }

pnpm format:check
if ($LASTEXITCODE -ne 0) { throw "frontend format check failed" }

$buildStartedUtc = [DateTime]::UtcNow
$oldRustFlags = $env:RUSTFLAGS
$cargoHome = [IO.Path]::GetFullPath((Join-Path $env:USERPROFILE ".cargo"))
$env:RUSTFLAGS = "--remap-path-prefix=$repoRoot=. --remap-path-prefix=$cargoHome=/cargo"
try {
    Write-Host "`n===== TAURI RELEASE BUILD ====="
    pnpm --dir apps/desktop exec -- tauri build --no-bundle --ci -- --locked
    if ($LASTEXITCODE -ne 0) { throw "Tauri release build failed" }
}
finally {
    $env:RUSTFLAGS = $oldRustFlags
}

$postBuildUnexpectedDirtyPaths = @(Get-UnexpectedDirtyPaths)
if ($postBuildUnexpectedDirtyPaths.Count -ne 0) {
    throw "release build mutated tracked source: $($postBuildUnexpectedDirtyPaths -join ', ')"
}

$artifacts = @()
if (Test-Path -LiteralPath $releaseExecutable) {
    $artifacts += Get-Item -LiteralPath $releaseExecutable
}
if (Test-Path -LiteralPath $bundleRoot) {
    $artifacts += Get-ChildItem -LiteralPath $bundleRoot -Recurse -File |
        Where-Object Extension -in @(".exe", ".msi")
}
$artifacts = @($artifacts | Where-Object LastWriteTimeUtc -ge $buildStartedUtc.AddSeconds(-5))
if ($artifacts.Count -eq 0) { throw "no fresh release artifact was produced" }

$timestamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ")
$outputDirectory = Join-Path $releaseRoot $timestamp
if (Test-Path -LiteralPath $outputDirectory) {
    throw "refusing to overwrite existing release evidence: $outputDirectory"
}
New-Item -ItemType Directory -Path $outputDirectory | Out-Null

$inventoryArtifacts = @(
    foreach ($artifact in $artifacts) {
        $destination = Join-Path $outputDirectory $artifact.Name
        Copy-Item -LiteralPath $artifact.FullName -Destination $destination
        $copied = Get-Item -LiteralPath $destination
        $signature = Get-SignatureState -Path $copied.FullName
        [PSCustomObject]@{
            relative_path = $copied.Name
            release_relative_path = "$timestamp/$($copied.Name)"
            absolute_path = $copied.FullName
            type = if ($copied.Name -eq "r2h-second-brain-app.exe") { "executable" } else { "installer" }
            bytes = $copied.Length
            last_modified_utc = $copied.LastWriteTimeUtc.ToString("o")
            sha256 = Get-Sha256 -Path $copied.FullName
            signature_status = [string]$signature.Status
            signer = [string]$signature.Signer
        }
    }
)

$configuration = Get-Content -Raw -LiteralPath "apps\desktop\src-tauri\tauri.conf.json" |
    ConvertFrom-Json
$tauriVersion = (cargo tree -p tauri --depth 0 | Select-Object -First 1) -replace "^tauri v", ""
if ([string]::IsNullOrWhiteSpace($tauriVersion)) { throw "Tauri version could not be resolved" }
$buildEndedUtc = [DateTime]::UtcNow
$inventory = [PSCustomObject]@{
    generated_utc = [DateTime]::UtcNow.ToString("o")
    repository_root = $repoRoot
    branch = (git branch --show-current)
    commit = (git rev-parse HEAD)
    product_name = $configuration.productName
    identifier = $configuration.identifier
    version = $configuration.version
    target_platform = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    target_architecture = $env:PROCESSOR_ARCHITECTURE
    target = "$([Runtime.InteropServices.RuntimeInformation]::OSDescription) / $env:PROCESSOR_ARCHITECTURE"
    tauri_version = $tauriVersion
    profile = "release"
    build_command = "pnpm --dir apps/desktop exec -- tauri build --no-bundle --ci -- --locked"
    build_started_utc = $buildStartedUtc.ToString("o")
    build_ended_utc = $buildEndedUtc.ToString("o")
    build_exit_code = 0
    bundle_active = [bool]$configuration.bundle.active
    artifacts = $inventoryArtifacts
}
$inventory | ConvertTo-Json -Depth 5 |
    Set-Content -LiteralPath (Join-Path $outputDirectory "release-inventory.json") -Encoding utf8

Write-Host "Release output: $outputDirectory"
$inventoryArtifacts | Format-Table type, relative_path, bytes, sha256, signature_status
Write-Host "`nKNOWLEDGE_CORE_RELEASE_BUILD_PASS"
