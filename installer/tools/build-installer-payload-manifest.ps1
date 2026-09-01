<#
.SYNOPSIS
  Builds the installer payload hash manifest (installer-payload-manifest.json)
  from the staged release payload.

.DESCRIPTION
  Records relative path, size, SHA-256, component, and version for the
  essential payload files. Essential files are re-verified after installation
  by verify-install.ps1 and gate the transactional activation.
#>
param(
    [Parameter(Mandatory)][string]$StageRoot,
    [Parameter(Mandatory)][string]$AppExe,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$OutputJson
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

$files = New-Object System.Collections.Generic.List[object]
function Add-File {
    param(
        [string]$Component, [string]$Root, [string]$FullPath,
        [string]$RelativePath, [bool]$Essential, [string]$FileVersion
    )
    if (-not (Test-Path $FullPath)) { throw "missing payload file: $FullPath" }
    $item = Get-Item $FullPath
    $files.Add([ordered]@{
        component  = $Component
        root       = $Root
        path       = $RelativePath
        sizeBytes  = $item.Length
        sha256     = Get-SHA256 $FullPath
        version    = $FileVersion
        essential  = $Essential
    })
}

# Application executable
$exeName = Split-Path $AppExe -Leaf
Add-File -Component "application" -Root "app" -FullPath $AppExe -RelativePath $exeName -Essential $true -FileVersion $Version

# AI pack: model manifest + models
$modelManifest = Get-Content (Join-Path $StageRoot "local-ai\manifests\r2h-multi-evidence-models.json") -Raw | ConvertFrom-Json
Add-File -Component "manifest" -Root "pack" `
    -FullPath (Join-Path $StageRoot "local-ai\manifests\r2h-multi-evidence-models.json") `
    -RelativePath "local-ai\manifests\r2h-multi-evidence-models.json" -Essential $true -FileVersion "1"

foreach ($role in @("generation", "embedding", "reranker")) {
    $entry = $modelManifest.models.$role
    $full = Join-Path $StageRoot ($entry.path -replace "/", "\")
    if ($entry.sha256) {
        Add-File -Component $role -Root "pack" -FullPath $full `
            -RelativePath ($entry.path -replace "/", "\") -Essential $true -FileVersion $entry.id
    }
    else {
        foreach ($name in $entry.expectedFiles) {
            Add-File -Component $role -Root "pack" -FullPath (Join-Path $full $name) `
                -RelativePath (($entry.path + "/" + $name) -replace "/", "\") `
                -Essential $true -FileVersion $entry.id
        }
    }
}

# llama.cpp runtime suite (llama-server + shared libraries)
Get-ChildItem (Join-Path $StageRoot "local-ai\runtimes") -File | ForEach-Object {
    $essential = $_.Name -in @("llama-server.exe", "llama-server-impl.dll", "llama.dll", "llama-common.dll", "ggml.dll", "ggml-base.dll", "mtmd.dll")
    Add-File -Component "llama-runtime" -Root "pack" -FullPath $_.FullName `
        -RelativePath ("local-ai\runtimes\" + $_.Name) -Essential $essential -FileVersion "llama.cpp"
}

# Packaged Python reranker runtime + worker
Add-File -Component "python-runtime" -Root "pack" `
    -FullPath (Join-Path $StageRoot "local-ai\python-runtime\python.exe") `
    -RelativePath "local-ai\python-runtime\python.exe" -Essential $true -FileVersion "3.12.13"
Add-File -Component "reranker-worker" -Root "pack" `
    -FullPath (Join-Path $StageRoot "local-ai\workers\reranker_worker.py") `
    -RelativePath "local-ai\workers\reranker_worker.py" -Essential $true -FileVersion $Version

# Khoj engine runtime
Add-File -Component "khoj-venv" -Root "appHome" `
    -FullPath (Join-Path $StageRoot "app-home\engines\khoj\.venv\Scripts\python.exe") `
    -RelativePath "engines\khoj\.venv\Scripts\python.exe" -Essential $true -FileVersion "3.11.0"
Add-File -Component "khoj-base-python" -Root "appHome" `
    -FullPath (Join-Path $StageRoot "app-home\engines\khoj\base-python311\python.exe") `
    -RelativePath "engines\khoj\base-python311\python.exe" -Essential $true -FileVersion "3.11.0"
Add-File -Component "khoj-wrapper" -Root "appHome" `
    -FullPath (Join-Path $StageRoot "app-home\scripts\run-khoj-windows.py") `
    -RelativePath "scripts\run-khoj-windows.py" -Essential $true -FileVersion $Version

$manifest = [ordered]@{
    product      = "r2h-second-brain"
    version      = $Version
    generatedUtc = (Get-Date).ToUniversalTime().ToString("o")
    roots        = [ordered]@{
        app     = "C:\Program Files\R2H\r2h-second-brain"
        pack    = "C:\ProgramData\R2H.AI-ELE"
        appHome = "C:\ProgramData\R2H\r2h-second-brain"
    }
    files        = $files
}

$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $OutputJson -Encoding utf8
Write-Host ("PAYLOAD_MANIFEST_WRITTEN files={0}" -f $files.Count)
