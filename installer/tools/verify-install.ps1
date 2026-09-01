<#
.SYNOPSIS
  R2H Second Brain - post-install self-check.

.DESCRIPTION
  Validates the installed application, AI pack manifest, model payloads,
  llama.cpp runtime, and the packaged Python reranker runtime.

  Modes:
    (default)     structural checks + essential hash verification
    -HashesOnly   essential hash verification only (installer transaction gate)
    -Full         QA mode: adds Python/torch/transformers imports,
                  llama-server version probe, and Khoj runtime import.

  Exit code 0 = all checks PASS, 1 = at least one FAIL.
#>
param(
    [switch]$Full,
    [switch]$HashesOnly,
    [switch]$Quiet,
    [string]$Json
)

$ErrorActionPreference = "Continue"

$PackRoot = "C:\ProgramData\R2H.AI-ELE"
$AppHome = "C:\ProgramData\R2H\r2h-second-brain"
$ModelManifestRelative = "local-ai\manifests\r2h-multi-evidence-models.json"
$PayloadManifestRelative = "payload-manifest.json"

$results = New-Object System.Collections.Generic.List[object]

function Add-Result {
    param([string]$Name, [bool]$Ok, [string]$Detail)
    $script:results.Add([ordered]@{
        name   = $Name
        status = if ($Ok) { "PASS" } else { "FAIL" }
        detail = $Detail
    })
    if (-not $Quiet) {
        $tag = if ($Ok) { "PASS" } else { "FAIL" }
        Write-Host ("{0} {1}: {2}" -f $tag, $Name, $Detail)
    }
}

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

# ---------------------------------------------------------------- app checks
$appExeCandidates = @(
    (Join-Path $AppHome "app\r2h-second-brain-app.exe"),
    "C:\Program Files\R2H\r2h-second-brain\r2h-second-brain-app.exe"
)
$appExe = $appExeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
Add-Result "application" ($null -ne $appExe) $(if ($appExe) { $appExe } else { "installed application executable not found" })

# ------------------------------------------------------------- model manifest
$manifestPath = Join-Path $PackRoot $ModelManifestRelative
$manifest = $null
if (Test-Path $manifestPath) {
    try {
        $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
        $rolesOk = $manifest.schemaVersion -eq 1 -and $manifest.offlineOnly -and
            $manifest.models.generation -and $manifest.models.embedding -and $manifest.models.reranker
        Add-Result "ai-manifest" ([bool]$rolesOk) $manifestPath
    }
    catch {
        Add-Result "ai-manifest" $false "invalid JSON: $manifestPath"
    }
}
else {
    Add-Result "ai-manifest" $false "missing: $manifestPath"
}

function Test-ModelFile {
    param($Entry, [string]$RoleName)
    $modelPath = Join-Path $PackRoot ($Entry.path -replace "/", "\")
    if (-not (Test-Path $modelPath)) {
        Add-Result $RoleName $false "missing: $($Entry.path)"
        return
    }
    if ($Entry.sha256) {
        if ($Entry.expectedSizeBytes -and (Get-Item $modelPath).Length -ne $Entry.expectedSizeBytes) {
            Add-Result $RoleName $false "size mismatch: $($Entry.path)"
            return
        }
        $actual = Get-SHA256 $modelPath
        if ($actual -ne $Entry.sha256.ToLower()) {
            Add-Result $RoleName $false "sha256 mismatch: $($Entry.path)"
            return
        }
        Add-Result $RoleName $true ("{0} (sha256 ok)" -f $Entry.id)
        return
    }
    if ($Entry.folderHashManifest) {
        foreach ($prop in $Entry.folderHashManifest.PSObject.Properties) {
            $filePath = Join-Path $modelPath ($prop.Name -replace "/", "\")
            if (-not (Test-Path $filePath)) {
                Add-Result $RoleName $false "missing: $($Entry.path)/$($prop.Name)"
                return
            }
            if ((Get-SHA256 $filePath) -ne $prop.Value.ToLower()) {
                Add-Result $RoleName $false "sha256 mismatch: $($Entry.path)/$($prop.Name)"
                return
            }
        }
        Add-Result $RoleName $true ("{0} (folder sha256 ok)" -f $Entry.id)
        return
    }
    Add-Result $RoleName $true $modelPath
}

if ($manifest -and -not $HashesOnly) {
    # Structural role checks
    Test-ModelFile $manifest.models.generation "generation-model" | Out-Null
}
if ($manifest) {
    if ($HashesOnly) {
        # Hash-only gate still needs the model payloads verified.
        Test-ModelFile $manifest.models.generation "generation-model" | Out-Null
    }
    Test-ModelFile $manifest.models.embedding "embedding-model" | Out-Null
    Test-ModelFile $manifest.models.reranker "reranker-model" | Out-Null
}

# --------------------------------------------------------------- runtimes
$llamaServer = Join-Path $PackRoot "local-ai\runtimes\llama-server.exe"
Add-Result "llama-runtime" (Test-Path $llamaServer) $llamaServer

$pythonExe = Join-Path $PackRoot "local-ai\python-runtime\python.exe"
$pythonOk = Test-Path $pythonExe
$pythonVersion = ""
if ($pythonOk) {
    try { $pythonVersion = (& $pythonExe --version 2>&1).ToString().Trim() } catch { $pythonOk = $false }
}
Add-Result "python-runtime" ($pythonOk -and $pythonVersion -match "3\.12\.13") "$pythonExe ($pythonVersion)"

$worker = Join-Path $PackRoot "local-ai\workers\reranker_worker.py"
Add-Result "reranker-worker" (Test-Path $worker) $worker

$khojPython = Join-Path $AppHome "engines\khoj\.venv\Scripts\python.exe"
Add-Result "khoj-runtime" (Test-Path $khojPython) $khojPython

# ------------------------------------------------ payload manifest (optional)
$payloadManifestPath = Join-Path $PackRoot $PayloadManifestRelative
if (Test-Path $payloadManifestPath) {
    try {
        $payload = Get-Content $payloadManifestPath -Raw | ConvertFrom-Json
        $rootMap = @{ pack = $PackRoot; appHome = $AppHome; app = "C:\Program Files\R2H\r2h-second-brain" }
        $bad = 0
        $checked = 0
        foreach ($file in $payload.files) {
            if (-not $file.essential) { continue }
            $rootPath = $rootMap[[string]$file.root]
            if (-not $rootPath) { continue }
            $full = Join-Path $rootPath ([string]$file.path)
            if (-not (Test-Path $full)) { $bad++; Write-Verbose "missing $full"; continue }
            $hash = Get-SHA256 $full
            $checked++
            if ($hash -ne ([string]$file.sha256).ToLower()) { $bad++ }
        }
        Add-Result "payload-manifest" ($bad -eq 0) "$checked essential files verified, $bad mismatches"
    }
    catch {
        Add-Result "payload-manifest" $false "unreadable: $payloadManifestPath"
    }
}

# ------------------------------------------------------------------- full QA
if ($Full) {
    try {
        $out = & $pythonExe -c "import torch, transformers, tokenizers, safetensors; print('TORCH=' + torch.__version__); print('TRANSFORMERS=' + transformers.__version__)" 2>&1
        $ok = ($LASTEXITCODE -eq 0) -and (($out | Out-String).Contains("TORCH="))
        Add-Result "torch-transformers" $ok (($out | Out-String).Trim())
    }
    catch {
        Add-Result "torch-transformers" $false $_.Exception.Message
    }
    try {
        $llamaOut = (& $llamaServer --version 2>&1 | Out-String).Trim()
        Add-Result "llama-server-version" ($LASTEXITCODE -eq 0) ($llamaOut -split "`n" | Select-Object -First 1)
    }
    catch {
        Add-Result "llama-server-version" $false $_.Exception.Message
    }
    try {
        $env:PYTHONPATH = Join-Path $AppHome "third_party\khoj\src"
        $khojOut = & $khojPython -c "import khoj.main; print('KHOJ_IMPORT_OK')" 2>&1
        Add-Result "khoj-import" (($khojOut | Out-String).Contains("KHOJ_IMPORT_OK")) (($khojOut | Out-String).Trim())
    }
    catch {
        Add-Result "khoj-import" $false $_.Exception.Message
    }
}

# -------------------------------------------------------------------- output
$failed = @($results | Where-Object { $_.status -eq "FAIL" })
if ($Json) {
    $summary = [ordered]@{
        generatedUtc = (Get-Date).ToUniversalTime().ToString("o")
        mode         = if ($Full) { "full" } elseif ($HashesOnly) { "hashes-only" } else { "standard" }
        overall      = if ($failed.Count -eq 0) { "PASS" } else { "FAIL" }
        results      = $results
    }
    $summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $Json -Encoding utf8
}
if (-not $Quiet) {
    Write-Host ""
    if ($failed.Count -eq 0) { Write-Host "INSTALL_VERIFICATION_PASS" }
    else { Write-Host ("INSTALL_VERIFICATION_FAIL ({0} failed)" -f $failed.Count) }
}
exit $(if ($failed.Count -eq 0) { 0 } else { 1 })
