param(
    [string]$BridgePath = "",
    [string]$Url = "https://example.com",
    [int]$TimeoutMs = 15000,
    [int]$RequestTimeoutMs = 0
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
if (-not $BridgePath) {
    $BridgePath = Join-Path $RepoRoot "target\release\codex-browser-bridge.exe"
}

if (-not (Test-Path -LiteralPath $BridgePath)) {
    Push-Location $RepoRoot
    try {
        cargo build --locked --release
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path -LiteralPath $BridgePath)) {
    throw "Bridge binary not found: $BridgePath"
}

if ($RequestTimeoutMs -le 0) {
    $RequestTimeoutMs = [Math]::Max($TimeoutMs + 5000, 10000)
}

Write-Host "Running doctor..."
$doctor = & $BridgePath --mode doctor
if ($LASTEXITCODE -ne 0) {
    throw "doctor failed with exit code $LASTEXITCODE"
}
$doctor | ConvertFrom-Json | Out-Null

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $BridgePath
$psi.ArgumentList.Add("--mode")
$psi.ArgumentList.Add("mcp")
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$psi.WorkingDirectory = $RepoRoot
$proc = [System.Diagnostics.Process]::Start($psi)

$nextId = 0
$tabId = $null
$bridgeStopped = $false
$stderrStarted = $false
$stderrState = [hashtable]::Synchronized(@{
    Buffer = [System.Text.StringBuilder]::new()
    MaxChars = 65536
    Sync = [object]::new()
})
$stderrSubscription = Register-ObjectEvent -InputObject $proc -EventName ErrorDataReceived -MessageData $stderrState -Action {
    if ($null -eq $EventArgs.Data) {
        return
    }
    $state = $Event.MessageData
    [System.Threading.Monitor]::Enter($state.Sync)
    try {
        [void]$state.Buffer.AppendLine($EventArgs.Data)
        if ($state.Buffer.Length -gt $state.MaxChars) {
            $remove = $state.Buffer.Length - $state.MaxChars
            [void]$state.Buffer.Remove(0, $remove)
        }
    } finally {
        [System.Threading.Monitor]::Exit($state.Sync)
    }
}
$proc.BeginErrorReadLine()
$stderrStarted = $true

function Stop-BridgeProcess {
    if ($script:bridgeStopped) {
        return
    }
    $script:bridgeStopped = $true
    if ($script:proc -and -not $script:proc.HasExited) {
        $script:proc.Kill()
        [void]$script:proc.WaitForExit(5000)
    }
}

function Read-BridgeStderr {
    if (-not $script:stderrState) {
        return ""
    }
    [System.Threading.Monitor]::Enter($script:stderrState.Sync)
    try {
        $text = $script:stderrState.Buffer.ToString().Trim()
    } finally {
        [System.Threading.Monitor]::Exit($script:stderrState.Sync)
    }
    if ($text) {
        return $text
    }
    return "<stderr empty>"
}

function Invoke-Mcp {
    param(
        [string]$Method,
        [object]$Params = $null
    )

    if ($script:bridgeStopped -or $script:proc.HasExited) {
        throw "MCP process is not running"
    }

    $script:nextId += 1
    $request = [ordered]@{
        jsonrpc = "2.0"
        id = $script:nextId
        method = $Method
    }
    if ($null -ne $Params) {
        $request.params = $Params
    }

    $json = $request | ConvertTo-Json -Depth 20 -Compress
    $script:proc.StandardInput.WriteLine($json)
    $script:proc.StandardInput.Flush()

    $lineTask = $script:proc.StandardOutput.ReadLineAsync()
    if (-not $lineTask.Wait($script:RequestTimeoutMs)) {
        Stop-BridgeProcess
        $stderr = Read-BridgeStderr
        throw "MCP response timed out after $script:RequestTimeoutMs ms for $Method. stderr: $stderr"
    }

    $line = $lineTask.Result
    if (-not $line) {
        if (-not $script:proc.HasExited) {
            Stop-BridgeProcess
        }
        $stderr = Read-BridgeStderr
        throw "MCP process closed before response. stderr: $stderr"
    }

    $response = $line | ConvertFrom-Json
    if ($response.error) {
        throw "MCP error $($response.error.code): $($response.error.message)"
    }
    if ($response.result.isError) {
        $text = ($response.result.content | ForEach-Object { $_.text }) -join "`n"
        throw "Tool error: $text"
    }
    return $response.result
}

function Invoke-Tool {
    param(
        [string]$Name,
        [hashtable]$Arguments = @{}
    )
    return Invoke-Mcp "tools/call" @{
        name = $Name
        arguments = $Arguments
    }
}

try {
    Invoke-Mcp "initialize" @{
        protocolVersion = "2024-11-05"
        capabilities = @{}
        clientInfo = @{ name = "codex-browser-bridge-live-e2e"; version = "0" }
    } | Out-Null

    $created = Invoke-Tool "codex_create_tab"
    $createdText = $created.content[0].text
    if ($createdText -notmatch "Created tab: (?<id>\S+)") {
        throw "Could not parse created tab id from: $createdText"
    }
    $tabId = $Matches.id

    Invoke-Tool "codex_nav_and_wait" @{
        tab_id = $tabId
        url = $Url
        timeout_ms = $TimeoutMs
    } | Out-Null

    $title = Invoke-Tool "codex_get_title" @{ tab_id = $tabId }
    $screenshot = Invoke-Tool "codex_screenshot" @{
        tab_id = $tabId
        format = "jpeg"
        quality = 60
    }

    $titleText = $title.content[0].text
    $image = $screenshot.content | Where-Object { $_.type -eq "image" } | Select-Object -First 1
    if (-not $image -or -not $image.data) {
        throw "Screenshot did not return image content"
    }

    Write-Host "Live E2E passed: tab=$tabId title=$titleText screenshot_base64_bytes=$($image.data.Length)"
} finally {
    if ($tabId -and -not $bridgeStopped -and -not $proc.HasExited) {
        try {
            Invoke-Tool "codex_close_tab" @{ tab_id = $tabId } | Out-Null
        } catch {
            Write-Warning $_
        }
    }
    if (-not $bridgeStopped -and -not $proc.HasExited) {
        try {
            Invoke-Tool "codex_finalize" | Out-Null
        } catch {
            Write-Warning $_
        }
    }
    Stop-BridgeProcess
    if ($stderrStarted) {
        try {
            $proc.CancelErrorRead()
        } catch {
            # Process may have exited while the async stderr reader was closing.
        }
    }
    if ($stderrSubscription) {
        Unregister-Event -SubscriptionId $stderrSubscription.Id -ErrorAction SilentlyContinue
        Remove-Job -Id $stderrSubscription.Id -Force -ErrorAction SilentlyContinue
    }
    $proc.Dispose()
}
