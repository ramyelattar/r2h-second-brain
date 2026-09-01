<#
.SYNOPSIS
  Live endpoint tests against the installed product's local AI services.
  Requires the installed application to be running (services on 42110-42113).
#>
param(
    [switch]$SkipGeneration
)

$ErrorActionPreference = "Continue"
$results = New-Object System.Collections.Generic.List[object]

function Add-Result {
    param([string]$Name, [bool]$Ok, [string]$Detail)
    $results.Add([ordered]@{ name = $Name; status = $(if ($Ok) { "PASS" } else { "FAIL" }); detail = $Detail })
    Write-Host ("{0} {1}: {2}" -f $(if ($Ok) { "PASS" } else { "FAIL" }), $Name, $Detail)
}

function Wait-Port {
    param([int]$Port, [int]$TimeoutSec = 600)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $conn = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue
        if ($conn) {
            $addr = ($conn | Select-Object -First 1).LocalAddress
            return $addr
        }
        Start-Sleep -Seconds 2
    }
    return $null
}

# --- 1. Generation (127.0.0.1:42111) ---
$addr = Wait-Port -Port 42111 -TimeoutSec 900
if ($addr -and $addr -eq "127.0.0.1") {
    try {
        $body = @{
            messages = @(@{ role = "user"; content = "Reply with exactly: R2H_GENERATION_OK" })
            max_tokens = 64
            temperature = 0
        } | ConvertTo-Json -Depth 4
        $resp = Invoke-RestMethod -Uri "http://127.0.0.1:42111/v1/chat/completions" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 300
        $text = $resp.choices[0].message.content
        $ok = -not [string]::IsNullOrWhiteSpace($text)
        Add-Result "generation" $ok ("127.0.0.1:42111 response: " + ($text -replace "`n", " ").Substring(0, [Math]::Min(120, $text.Length)))
    }
    catch {
        Add-Result "generation" $false $_.Exception.Message
    }
}
else {
    Add-Result "generation" $false "port 42111 not listening on 127.0.0.1 (addr=$addr)"
}

# --- 2. Embedding (127.0.0.1:42112), dimension 1024 ---
$addr = Wait-Port -Port 42112 -TimeoutSec 900
if ($addr -and $addr -eq "127.0.0.1") {
    try {
        $body = @{ input = @("R2H offline embedding probe") } | ConvertTo-Json
        $resp = Invoke-RestMethod -Uri "http://127.0.0.1:42112/v1/embeddings" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 300
        $vec = @($resp.data[0].embedding)
        $dim = $vec.Count
        $finite = ($vec | Where-Object { [double]::IsNaN($_) -or [double]::IsInfinity($_) }).Count -eq 0
        $nonZero = ($vec | Where-Object { $_ -ne 0 }).Count -gt 0
        Add-Result "embedding" ($dim -eq 1024 -and $finite -and $nonZero) "dimension=$dim finite=$finite nonzero=$nonZero"
    }
    catch {
        Add-Result "embedding" $false $_.Exception.Message
    }
}
else {
    Add-Result "embedding" $false "port 42112 not listening on 127.0.0.1"
}

# --- 3. Reranker (127.0.0.1:42113) ---
$addr = Wait-Port -Port 42113 -TimeoutSec 900
if ($addr -and $addr -eq "127.0.0.1") {
    try {
        $health = Invoke-RestMethod -Uri "http://127.0.0.1:42113/health" -TimeoutSec 120
        Add-Result "reranker-health" ($health.status -eq "ok") ("model=" + $health.model + " offline=" + $health.offline)
        $prompt = "<fmt:query>What affects feeder voltage drop?</fmt:query><fmt:document>Feeder voltage drop depends on current and conductor resistance.</fmt:document>"
        $body = @{ prompt = $prompt } | ConvertTo-Json
        $resp = Invoke-RestMethod -Uri "http://127.0.0.1:42113/completion" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 300
        $yeslp = $resp.tokens | Where-Object { $_.tok_str -eq "Yes" } | Select-Object -First 1
        $nolp = $resp.tokens | Where-Object { $PSItem.tok_str -eq "No" } | Select-Object -First 1
        $ok = ($null -ne $yeslp) -and ($null -ne $nolp) -and ($yeslp.logprob -gt $nolp.logprob)
        Add-Result "reranker" $ok ("model=" + $resp.model + " yes_logprob=" + [math]::Round($yeslp.logprob, 4) + " no_logprob=" + [math]::Round($nolp.logprob, 4))
    }
    catch {
        Add-Result "reranker" $false $_.Exception.Message
    }
}
else {
    Add-Result "reranker" $false "port 42113 not listening on 127.0.0.1"
}

# --- 4. Khoj Intelligence Core (127.0.0.1:42110) ---
$addr = Wait-Port -Port 42110 -TimeoutSec 900
if ($addr -and $addr -eq "127.0.0.1") {
    try {
        $resp = Invoke-WebRequest -Uri "http://127.0.0.1:42110/" -TimeoutSec 60 -UseBasicParsing
        Add-Result "khoj" ($resp.StatusCode -eq 200) "Khoj ready on 127.0.0.1:42110"
        # Real generation through Khoj -> llama.cpp (42111). Khoj proxies the
        # local OpenAI-compatible endpoint configured by run-khoj-windows.py.
        try {
            $chatBody = @{ q = "Reply with exactly: R2H_KHOJ_CHAT_OK" } | ConvertTo-Json
            $chat = Invoke-WebRequest -Uri "http://127.0.0.1:42110/api/chat?client=r2h-test" -Method Post -Body $chatBody -ContentType "application/json" -TimeoutSec 300 -UseBasicParsing
            Add-Result "khoj-chat" ($chat.StatusCode -eq 200) ("HTTP " + $chat.StatusCode + " (generation proxied through Khoj)")
        }
        catch {
            $status = $null
            if ($_.Exception.Response) { $status = [int]$_.Exception.Response.StatusCode }
            Add-Result "khoj-chat" $false "HTTP $status $($_.Exception.Message)"
        }
    }
    catch {
        Add-Result "khoj" $false $_.Exception.Message
    }
}
else {
    Add-Result "khoj" $false "port 42110 not listening on 127.0.0.1 (Khoj may still be starting)"
}

# --- 5. Loopback-only assertion for all ports ---
foreach ($port in @(42110, 42111, 42112, 42113)) {
    $conns = Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue
    $nonLoopback = @($conns | Where-Object { $_.LocalAddress -notin @("127.0.0.1", "::1") })
    Add-Result "loopback-only-$port" ($nonLoopback.Count -eq 0) $(if ($nonLoopback.Count -eq 0) { "bound to 127.0.0.1 only" } else { "NON-LOOPBACK BINDING FOUND" })
}

$failed = @($results | Where-Object { $_.status -eq "FAIL" })
Write-Host ""
if ($failed.Count -eq 0) { Write-Host "LIVE_ENDPOINT_TESTS_PASS" } else { Write-Host ("LIVE_ENDPOINT_TESTS_FAIL ({0})" -f $failed.Count) }
exit $(if ($failed.Count -eq 0) { 0 } else { 1 })
