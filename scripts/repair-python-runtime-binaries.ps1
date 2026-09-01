$ErrorActionPreference = "Continue"
$pack = "C:\ProgramData\R2H.AI-ELE\local-ai\python-runtime"

$f = Get-Item "$pack\python.exe"
Write-Output "python.exe Attributes: $($f.Attributes) IsReadOnly: $($f.IsReadOnly) Length: $($f.Length)"

$lockers = Get-Process | Where-Object { $_.Path -like "$pack*" }
if ($lockers) { $lockers | Select-Object Id, ProcessName, Path | Format-Table | Out-String | Write-Output }
else { Write-Output "NO_RUNNING_PROCESS_FROM_PACK" }

# Clear read-only flag and retry the binary copies
foreach ($name in @("python.exe", "pythonw.exe", "python3.dll", "python312.dll", "vcruntime140.dll", "vcruntime140_1.dll")) {
    $target = "$pack\$name"
    if (Test-Path $target) { Set-ItemProperty $target -Name IsReadOnly -Value $false -ErrorAction SilentlyContinue }
    try {
        Copy-Item "D:\uv\python\cpython-3.12.13-windows-x86_64-none\$name" $target -Force -ErrorAction Stop
        Write-Output "COPIED $name"
    } catch {
        Write-Output "FAILED $name : $($_.Exception.Message)"
    }
}

& "$pack\python.exe" -c "import sys; print('python', sys.version.split()[0])"
& "$pack\python.exe" -c "import re; print('stdlib OK')"
& "$pack\python.exe" -c "import torch; print('torch', torch.__version__)"
& "$pack\python.exe" -c "import transformers; print('transformers', transformers.__version__)"
& "$pack\python.exe" -c "import tokenizers, safetensors; print('tokenizers+safetensors OK')"
Write-Output "DIAGNOSTIC_FINISHED"
