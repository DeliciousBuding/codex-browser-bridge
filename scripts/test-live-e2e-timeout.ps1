param(
    [int]$RequestTimeoutMs = 700,
    [int]$MaxElapsedSeconds = 10
)

$ErrorActionPreference = "Stop"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("codex-bridge-fake-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null

$source = Join-Path $tmp "fake_bridge.rs"
$exe = Join-Path $tmp "fake_bridge.exe"

@'
fn main() {
    use std::io::BufRead;

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--mode" && args[2] == "doctor" {
        println!("{{}}");
        return;
    }
    for i in 0..10_000 {
        eprintln!("fake bridge stderr flood line {i}: {}", "x".repeat(256));
    }
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    if lines.next().is_some() {
        println!(r#"{{"jsonrpc":"2.0","id":1,"result":{{}}}}"#);
    }
    if lines.next().is_some() {
        println!(r#"{{"jsonrpc":"2.0","id":2,"result":{{"content":[{{"type":"text","text":"Created tab: 7"}}]}}}}"#);
    }
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
'@ | Set-Content -LiteralPath $source -Encoding ASCII

try {
    rustc $source -o $exe

    $elapsed = [Diagnostics.Stopwatch]::StartNew()
    try {
        & (Join-Path $PSScriptRoot "live-e2e.ps1") `
            -BridgePath $exe `
            -TimeoutMs 1000 `
            -RequestTimeoutMs $RequestTimeoutMs
        throw "fake bridge unexpectedly passed live E2E"
    } catch {
        $elapsed.Stop()
        if ($_.Exception.Message -notlike "*MCP response timed out*") {
            throw
        }
        if ($_.Exception.Message -notlike "*for tools/call*") {
            throw "stderr drain did not let the fake bridge reach the post-initialize tool timeout: $($_.Exception.Message)"
        }
        if ($elapsed.Elapsed.TotalSeconds -gt $MaxElapsedSeconds) {
            throw "timeout cleanup took too long: $($elapsed.Elapsed.TotalSeconds)s"
        }
        Write-Host "Live E2E timeout cleanup passed in $([Math]::Round($elapsed.Elapsed.TotalSeconds, 2))s"
    }
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
