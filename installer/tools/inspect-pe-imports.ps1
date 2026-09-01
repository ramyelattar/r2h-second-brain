param(
    [Parameter(Mandatory)][string[]]$Path
)
# Minimal PE import-table reader: prints DLL names each image imports.
foreach ($file in $Path) {
    Write-Host "=== $file ==="
    if (-not (Test-Path $file)) { Write-Host "MISSING"; continue }
    $fs = [IO.File]::OpenRead($file)
    try {
        $br = New-Object IO.BinaryReader($fs)
        if ($br.ReadUInt16() -ne 0x5A4D) { Write-Host "NOT_PE"; continue }
        $fs.Seek(0x3C, 'Begin') | Out-Null
        $peOffset = $br.ReadInt32()
        $fs.Seek($peOffset, 'Begin') | Out-Null
        if ($br.ReadUInt32() -ne 0x00004550) { Write-Host "NOT_PE"; continue }
        $fs.Seek($peOffset + 6, 'Begin') | Out-Null
        $numSections = $br.ReadUInt16()
        $fs.Seek($peOffset + 24, 'Begin') | Out-Null
        $optionalMagic = $br.ReadUInt16()
        $pe32plus = $optionalMagic -eq 0x20B
        if ($pe32plus) { $sectionTableOffset = $peOffset + 24 + 240 } else { $sectionTableOffset = $peOffset + 24 + 224 }
        $dataDirOffset = $peOffset + 24 + $(if ($pe32plus) { 112 } else { 96 })
        $fs.Seek($dataDirOffset + 8, 'Begin') | Out-Null
        $importRva = $br.ReadUInt32()
        if ($importRva -eq 0) { Write-Host "NO_IMPORTS"; continue }
        $sections = @()
        $fs.Seek($sectionTableOffset, 'Begin') | Out-Null
        for ($i = 0; $i -lt $numSections; $i++) {
            $br.ReadBytes(8) | Out-Null
            $virtSize = $br.ReadUInt32()
            $virtAddr = $br.ReadUInt32()
            $rawSize = $br.ReadUInt32()
            $rawPtr = $br.ReadUInt32()
            $br.ReadBytes(16) | Out-Null
            $sections += [pscustomobject]@{ VA = $virtAddr; VS = $virtSize; RP = $rawPtr; RS = $rawSize }
        }
        function RvaToOff([uint32]$rva) {
            foreach ($s in $script:sections) {
                if ($rva -ge $s.VA -and $rva -lt ($s.VA + [Math]::Max($s.VS, $s.RS))) {
                    return $s.RP + ($rva - $s.VA)
                }
            }
            return -1
        }
        $descOff = RvaToOff ([uint32]$importRva)
        if ($descOff -lt 0) { Write-Host "IMPORTS_UNREADABLE"; continue }
        for ($d = 0; ; $d++) {
            $fs.Seek(($descOff + 20 * $d), 'Begin') | Out-Null
            $originalThunk = $br.ReadUInt32()
            $timeStamp = $br.ReadUInt32()
            $forwarder = $br.ReadUInt32()
            $nameRva = $br.ReadUInt32()
            $firstThunk = $br.ReadUInt32()
            if ($nameRva -eq 0) { break }
            $nameOff = RvaToOff ([uint32]$nameRva)
            if ($nameOff -lt 0) { continue }
            $fs.Seek($nameOff, 'Begin') | Out-Null
            $chars = New-Object Text.StringBuilder
            while ($true) {
                $ch = $br.ReadByte()
                if ($ch -eq 0) { break }
                [void]$chars.Append([char]$ch)
            }
            Write-Host $chars.ToString()
        }
    }
    finally { $fs.Dispose() }
}
