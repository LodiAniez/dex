# Installs Dex's Claude Code hooks (PRD §9.3) into your Claude Code settings,
# %USERPROFILE%\.claude\settings.json by default. The merge is done by
# `dex hooks install`: your other hooks and settings stay exactly as they are,
# the file as it was before Dex's first install is kept next to it
# (settings.json.dex-backup), and running this again is safe.
#
#   .\scripts\install-hooks.ps1                       # your real settings
#   .\scripts\install-hooks.ps1 -Settings .\try.json  # any other file, to try it out
#   .\scripts\install-hooks.ps1 -Uninstall            # remove Dex's hooks only
#
# The hooks run the dex.exe that installs them, so re-run this if dex.exe moves.

param(
    [string]$Settings,
    [string]$Dex,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
if (-not $Dex) {
    $onPath = Get-Command dex.exe -ErrorAction SilentlyContinue
    $Dex = if ($onPath) { $onPath.Source } else { Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release\dex.exe' }
}
if (-not (Test-Path $Dex)) {
    throw "dex.exe not found at $Dex. Build it (cargo build --release -p dex-cli) or pass -Dex <path>."
}

$action = if ($Uninstall) { 'uninstall' } else { 'install' }
$arguments = @('hooks', $action)
if ($Settings) { $arguments += @('--settings', $Settings) }
& $Dex @arguments
exit $LASTEXITCODE
