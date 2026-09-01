<#
.SYNOPSIS
  Builds release-manifest.json for the release-output directory: every
  artifact with size and SHA-256, plus product metadata.
#>
param(
    [Parameter(Mandatory)][string]$ReleaseOutput,
    [Parameter(Mandatory)][string]$Version,
    [string]$Commit = "",
    [string]$Branch = ""
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

$artifacts = @()
Get-ChildItem $ReleaseOutput -File | ForEach-Object {
    $artifacts += [ordered]@{
        name             = $_.Name
        bytes            = $_.Length
        sha256           = Get-SHA256 $_.FullName
        lastModifiedUtc  = $_.LastWriteTimeUtc.ToString("o")
    }
}

$manifest = [ordered]@{
    product       = "r2h-second-brain"
    version       = $Version
    generatedUtc  = (Get-Date).ToUniversalTime().ToString("o")
    platform      = "windows-x64"
    branch        = $Branch
    commit        = $Commit
    installerAppId= "{3151A1D6-1BAA-4D1D-B8CA-8CBF69899338}"
    artifacts     = $artifacts
}

$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $ReleaseOutput "release-manifest.json") -Encoding utf8
Write-Host ("RELEASE_MANIFEST_WRITTEN artifacts={0}" -f $artifacts.Count)
