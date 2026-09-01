# Repairs the corrupt ProgramData python-runtime (mixed 3.10.5/3.12.13 stdlib,
# missing DLLs\_sre) by rebuilding the interpreter layer from the local uv
# cpython-3.12.13 installation. Site-packages (cp312 torch/transformers) is
# preserved. All replaced state is kept in *.corrupt-20260831 backup folders.
# Fail-closed: aborts before mutating anything unless every precondition holds.

$ErrorActionPreference = "Stop"

$pack = "C:\ProgramData\R2H.AI-ELE\local-ai\python-runtime"
$src  = "D:\uv\python\cpython-3.12.13-windows-x86_64-none"
$stamp = "corrupt-20260831"

function Fail($message) {
    Write-Output "ABORT: $message"
    exit 1
}

# --- Preconditions (fail-closed, nothing mutated yet) ---
if (-not (Test-Path "$pack\python.exe")) { Fail "pack python.exe missing" }
if (-not (Test-Path "$pack\lib\site-packages\torch")) { Fail "pack site-packages\torch missing" }
if (-not (Test-Path "$src\python.exe")) { Fail "uv source python.exe missing" }
$srcVersion = & "$src\python.exe" --version 2>&1
if ($srcVersion -notlike "*3.12.13*") { Fail "uv source is not 3.12.13: $srcVersion" }
if (Test-Path "$pack\Lib.$stamp") { Fail "backup folder already exists; refusing to rerun" }
if (Test-Path "$pack\DLLs.$stamp") { Fail "DLLs backup folder already exists; refusing to rerun" }
Write-Output "PRECONDITIONS_OK"

# --- 1. Back up the corrupt interpreter layer ---
Rename-Item "$pack\Lib"  "Lib.$stamp"
Rename-Item "$pack\DLLs" "DLLs.$stamp"
Write-Output "BACKUP_DONE Lib.$stamp DLLs.$stamp"

# --- 2. Fresh 3.12.13 stdlib (without site-packages or caches) ---
$libTree = robocopy "$src\Lib" "$pack\Lib" /E /XD site-packages __pycache__ /NFL /NDL /NJH /NP
if ($LASTEXITCODE -ge 8) { Fail "robocopy Lib failed with $LASTEXITCODE" }
Write-Output "STDLIB_COPIED"

# --- 3. Restore the existing site-packages into the fresh stdlib tree ---
Move-Item "$pack\Lib.$stamp\site-packages" "$pack\Lib\site-packages"
Write-Output "SITE_PACKAGES_RESTORED"

# --- 4. Fresh stdlib extension modules ---
$dllTree = robocopy "$src\DLLs" "$pack\DLLs" /E /NFL /NDL /NJH /NP
if ($LASTEXITCODE -ge 8) { Fail "robocopy DLLs failed with $LASTEXITCODE" }
Write-Output "EXTENSION_MODULES_COPIED"

# --- 5. Interpreter binaries (python310.dll intentionally left for forensics) ---
foreach ($name in @("python.exe", "pythonw.exe", "python3.dll", "python312.dll", "vcruntime140.dll", "vcruntime140_1.dll")) {
    Copy-Item "$src\$name" "$pack\$name" -Force
}
Write-Output "BINARIES_COPIED"

# --- 6. Verification ---
$py = "$pack\python.exe"
& $py -c "import sys; print('python', sys.version.split()[0])"
& $py -c "import re, json; print('stdlib OK')"
& $py -c "import torch; print('torch', torch.__version__)"
& $py -c "import transformers; print('transformers', transformers.__version__)"
& $py -c "import tokenizers, safetensors; print('tokenizers+safetensors OK')"
Write-Output "REPAIR_SCRIPT_FINISHED"
