# Measures what every Claude Code hook invocation costs (PRD §9.3):
#   no-op      spawn dex.exe outside a Dex pane; it returns before parsing arguments.
#              Paid on every prompt and tool batch by every unrelated session.
#   round trip spawn dex.exe, connect to the running app, handshake, one request.
#              The floor for any hook that talks to the daemon. Needs Dex running.
# Build first: cargo build --release -p dex-cli

param([int]$Runs = 100)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dex = Join-Path $root 'target\release\dex.exe'
if (-not (Test-Path $dex)) { throw "Missing $dex. Run: cargo build --release -p dex-cli" }

function Get-Stats([double[]]$ms) {
    $sorted = $ms | Sort-Object
    $p95Index = [math]::Min($sorted.Count - 1, [int][math]::Ceiling($sorted.Count * 0.95) - 1)
    [pscustomobject]@{
        Median = [math]::Round($sorted[[int]($sorted.Count / 2)], 1)
        P95    = [math]::Round($sorted[$p95Index], 1)
        Min    = [math]::Round($sorted[0], 1)
        Max    = [math]::Round($sorted[-1], 1)
    }
}

function Measure-Runs([scriptblock]$body) {
    & $body | Out-Null   # warm-up: the first run pays the disk read and antivirus scan
    foreach ($i in 1..$Runs) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        & $body | Out-Null
        $sw.Stop()
        $sw.Elapsed.TotalMilliseconds
    }
}

Remove-Item Env:DEX_PANE_ID -ErrorAction SilentlyContinue
$noop = Measure-Runs { & $dex event stop }

& $dex workspace list --json | Out-Null
$appRunning = $LASTEXITCODE -eq 0
$roundTrip = if ($appRunning) { Measure-Runs { & $dex workspace list --json } } else { @() }

$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
$av = try { if ((Get-MpComputerStatus).RealTimeProtectionEnabled) { 'Defender real-time on' } else { 'Defender real-time off' } } catch { 'unknown' }
"Machine: $cpu | $av | $Runs runs each | $(Get-Date -Format 'yyyy-MM-dd')"
$rows = @([pscustomobject]@{ Path = 'no-op (outside a pane)'; Stats = Get-Stats $noop })
if ($appRunning) {
    $rows += [pscustomobject]@{ Path = 'round trip (handshake + workspace.list)'; Stats = Get-Stats $roundTrip }
} else {
    "Dex is not running: round trip skipped. Start the app and run this again."
}
$rows | Select-Object Path, @{ n = 'Median ms'; e = { $_.Stats.Median } }, @{ n = 'P95 ms'; e = { $_.Stats.P95 } }, @{ n = 'Min ms'; e = { $_.Stats.Min } }, @{ n = 'Max ms'; e = { $_.Stats.Max } }
