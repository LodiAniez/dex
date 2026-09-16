# Connects Dex to Claude Code: the hooks that report agent status (PRD §9.3)
# and the MCP server that shares workspace context (§10.2).
#
# Both halves are done by the dex binaries, not by this script. `dex hooks
# install` merges into %USERPROFILE%\.claude\settings.json, leaving your other
# hooks and settings exactly as they are and keeping the file as it was before
# Dex's first install beside it (settings.json.dex-backup). `dex mcp install`
# registers through `claude mcp add --scope user` and never edits
# ~/.claude.json by hand — that file also holds your login session.
#
#   .\scripts\install-hooks.ps1                       # hooks and MCP server
#   .\scripts\install-hooks.ps1 -Settings .\try.json  # hooks into another file, to try it out
#   .\scripts\install-hooks.ps1 -SkipMcp              # hooks only
#   .\scripts\install-hooks.ps1 -Uninstall            # remove both again
#
# Everything here is safe to run twice. Both halves run the binaries that
# install them, so re-run this if they move.

param(
    [string]$Settings,
    [string]$Dex,
    [switch]$SkipMcp,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
if (-not $Dex) {
    $onPath = Get-Command dex.exe -ErrorAction SilentlyContinue
    $Dex = if ($onPath) { $onPath.Source } else { Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release\dex.exe' }
}
if (-not (Test-Path $Dex)) {
    throw "dex.exe not found at $Dex. Build it (cargo build --release) or pass -Dex <path>."
}

$action = if ($Uninstall) { 'uninstall' } else { 'install' }

$arguments = @('hooks', $action)
if ($Settings) { $arguments += @('--settings', $Settings) }
& $Dex @arguments
$failed = $LASTEXITCODE

# A throwaway -Settings file means you are trying the hooks out; registering the
# MCP server for real at the same time would be a surprise.
if ($SkipMcp -or $Settings) {
    if ($Settings -and -not $SkipMcp) { "Skipped the MCP server: -Settings means this was a trial run." }
    exit $failed
}

& $Dex mcp $action
if ($LASTEXITCODE -ne 0) { $failed = $LASTEXITCODE }
exit $failed
