$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
Set-Location $repoRoot

function Invoke-Checked {
    param(
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][scriptblock]$Command
    )

    Write-Host "`n===== $Label ====="
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$Label failed" }
}

Invoke-Checked -Label "CONTROLLED CORE OFFLINE LIFECYCLE" -Command {
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
        (Join-Path $PSScriptRoot "verify-offline.ps1")
}
Invoke-Checked -Label "RELEASE E2E LIFECYCLE" -Command {
    cargo test --release -p knowledge-e2e `
        --test e2e_ingest_search `
        --test e2e_restart_recovery `
        --test e2e_workspace_isolation `
        --test e2e_audit_integrity
}
Invoke-Checked -Label "RELEASE BACKUP AND RESTORE" -Command {
    cargo test --release -p knowledge-app --test maintenance -- --nocapture
}

Write-Host "`nKNOWLEDGE_CORE_RELEASE_SMOKE_PASS"
