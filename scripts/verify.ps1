$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Write-Host "`n===== RUST FORMAT ====="
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed" }

Write-Host "`n===== RUST CLIPPY ====="
cargo clippy --workspace --all-targets --all-features -- -D warnings
if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }

Write-Host "`n===== RUST TESTS ====="
cargo test --workspace --all-features
if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

if (Test-Path ".\\apps\\desktop\\package.json") {
    Write-Host "`n===== FRONTEND INSTALL ====="
    pnpm install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw "pnpm install failed" }

    Write-Host "`n===== FRONTEND CHECKS ====="
    pnpm lint
    if ($LASTEXITCODE -ne 0) { throw "pnpm lint failed" }

    $testOutput = Join-Path $PSScriptRoot "..\test-output\frontend-verification"
    New-Item -ItemType Directory -Force -Path $testOutput | Out-Null
    $testTimestamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ")
    $testStdout = Join-Path $testOutput "$testTimestamp.stdout.txt"
    $testStderr = Join-Path $testOutput "$testTimestamp.stderr.txt"
    $testProcess = Start-Process `
        -FilePath (Get-Command pnpm.cmd -ErrorAction Stop).Source `
        -ArgumentList "test", "--", "--run" `
        -WorkingDirectory (Get-Location).Path `
        -PassThru `
        -Wait `
        -WindowStyle Hidden `
        -RedirectStandardOutput $testStdout `
        -RedirectStandardError $testStderr
    Get-Content -Raw -LiteralPath $testStdout
    Get-Content -Raw -LiteralPath $testStderr
    if ($testProcess.ExitCode -ne 0) { throw "pnpm test failed" }

    pnpm build
    if ($LASTEXITCODE -ne 0) { throw "pnpm build failed" }
}

Write-Host "`nKNOWLEDGE_CORE_VERIFICATION_PASS"
