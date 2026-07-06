param(
    [string]$BridgePath = "",
    [string]$Url = "https://example.com",
    [int]$TimeoutMs = 15000
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

function Invoke-Mcp {
    param(
        [string]$Method,
        [object]$Params = $null
    )

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

    $line = $script:proc.StandardOutput.ReadLine()
    if (-not $line) {
        $stderr = $script:proc.StandardError.ReadToEnd()
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
    if ($tabId) {
        try {
            Invoke-Tool "codex_close_tab" @{ tab_id = $tabId } | Out-Null
        } catch {
            Write-Warning $_
        }
    }
    try {
        Invoke-Tool "codex_finalize" | Out-Null
    } catch {
        Write-Warning $_
    }
    if (-not $proc.HasExited) {
        $proc.Kill()
    }
    $proc.Dispose()
}
