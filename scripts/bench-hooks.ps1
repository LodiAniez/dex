# Measures what every Claude Code hook invocation costs (PRD §9.3, milestone M0):
#   no-op      spawn dex.exe outside a Dex pane; it returns before parsing arguments.
#              Paid on every prompt and tool batch by every unrelated session.
#   round trip spawn dex.exe, open a named pipe, send one request, read one response.
#              The floor for any hook that talks to the daemon. The handshake is
#              added in M4 and this is re-measured then.
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

# --- no-op path -------------------------------------------------------------
Remove-Item Env:DEX_PANE_ID -ErrorAction SilentlyContinue
& $dex event stop   # warm-up: the first run pays the disk read and antivirus scan
$noop = foreach ($i in 1..$Runs) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & $dex event stop
    $sw.Stop()
    $sw.Elapsed.TotalMilliseconds
}

# --- round trip -------------------------------------------------------------
# An echo server on a background runspace. It signals $ready each time a new
# pipe instance exists, so no client spawn ever races an instance that isn't
# listening yet, and the wait stays outside the timed region.
$pipeName = "dex-bench-$PID"
$ready = New-Object System.Threading.AutoResetEvent $false
$server = [powershell]::Create().AddScript({
    param($name, $count, $ready)
    for ($i = 0; $i -lt $count; $i++) {
        $stream = New-Object System.IO.Pipes.NamedPipeServerStream($name, [System.IO.Pipes.PipeDirection]::InOut, 1)
        [void]$ready.Set()
        $stream.WaitForConnection()
        $reader = New-Object System.IO.StreamReader($stream)
        $writer = New-Object System.IO.StreamWriter($stream)
        [void]$reader.ReadLine()
        $writer.WriteLine('{"id":"bench","ok":true,"data":{}}')
        $writer.Flush()
        $stream.WaitForPipeDrain()
        $stream.Dispose()
    }
}).AddArgument($pipeName).AddArgument($Runs + 1).AddArgument($ready)
$handle = $server.BeginInvoke()

try {
    [void]$ready.WaitOne(5000)
    & $dex bench-ping --pipe $pipeName   # warm-up
    if ($LASTEXITCODE -ne 0) { throw "bench-ping failed on warm-up" }

    $roundTrip = foreach ($i in 1..$Runs) {
        if (-not $ready.WaitOne(5000)) { throw "echo server stopped responding" }
        $sw = [Diagnostics.Stopwatch]::StartNew()
        & $dex bench-ping --pipe $pipeName
        $sw.Stop()
        if ($LASTEXITCODE -ne 0) { throw "bench-ping failed on run $i" }
        $sw.Elapsed.TotalMilliseconds
    }
} finally {
    [void]$server.EndInvoke($handle)
    $server.Dispose()
}

# --- report -----------------------------------------------------------------
$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
$av = try { if ((Get-MpComputerStatus).RealTimeProtectionEnabled) { 'Defender real-time on' } else { 'Defender real-time off' } } catch { 'unknown' }
"Machine: $cpu | $av | $Runs runs each | $(Get-Date -Format 'yyyy-MM-dd')"
[pscustomobject]@{ Path = 'no-op (outside a pane)' } | Select-Object Path, @{ n = 'Median ms'; e = { (Get-Stats $noop).Median } }, @{ n = 'P95 ms'; e = { (Get-Stats $noop).P95 } }, @{ n = 'Min ms'; e = { (Get-Stats $noop).Min } }, @{ n = 'Max ms'; e = { (Get-Stats $noop).Max } }
[pscustomobject]@{ Path = 'round trip (pipe, no handshake)' } | Select-Object Path, @{ n = 'Median ms'; e = { (Get-Stats $roundTrip).Median } }, @{ n = 'P95 ms'; e = { (Get-Stats $roundTrip).P95 } }, @{ n = 'Min ms'; e = { (Get-Stats $roundTrip).Min } }, @{ n = 'Max ms'; e = { (Get-Stats $roundTrip).Max } }
