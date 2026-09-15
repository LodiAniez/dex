# Warns about source files over the size limit (docs/conventions.md §4.2).
# A nudge, not a wall: always exits 0 so a justified large file can exist.

$limit = 400
$root = Split-Path -Parent $PSScriptRoot
$dirs = @("$root\crates", "$root\app\src", "$root\app\src-tauri\src") | Where-Object { Test-Path $_ }

$over = Get-ChildItem -Path $dirs -Recurse -File -Include *.rs, *.ts, *.tsx |
    Where-Object { $_.FullName -notmatch '\\(target|node_modules|generated)\\' } |
    ForEach-Object {
        $lines = @(Get-Content -LiteralPath $_.FullName).Count
        if ($lines -gt $limit) {
            [pscustomobject]@{ Lines = $lines; File = $_.FullName.Substring($root.Length + 1) }
        }
    }

if ($over) {
    Write-Warning "Files over $limit lines (split them, or say why not):"
    $over | Sort-Object Lines -Descending | Format-Table -AutoSize | Out-String | Write-Host
} else {
    Write-Host "All source files are within $limit lines."
}
exit 0
