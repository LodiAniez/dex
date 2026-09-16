# Renders the Dex wordmark `</dex>` as a 1024x1024 PNG: a dark rounded tile,
# the angle brackets and slash in the app's accent blue, the name in its text
# colour. Same palette as the UI (app/src/shell/styles.css).
param(
    [string]$Out = 'C:\Users\Admin\AppData\Local\Temp\claude\C--Users-Admin-Desktop-projects-dex\50dd2c31-8882-4b9b-8235-001b8245f3b5\scratchpad\logo.png',
    [string]$FontName = 'Cascadia Code',
    [int]$Size = 1024
)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap $Size, $Size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = 'AntiAlias'
$g.TextRenderingHint = 'AntiAliasGridFit'
$g.InterpolationMode = 'HighQualityBicubic'
$g.Clear([System.Drawing.Color]::Transparent)

# Tile: rounded square with a little inset so the corners breathe in a taskbar.
$inset = [int]($Size * 0.04)
$radius = [int]($Size * 0.22)
$rect = New-Object System.Drawing.Rectangle $inset, $inset, ($Size - 2 * $inset), ($Size - 2 * $inset)
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$d = $radius * 2
$path.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
$path.AddArc($rect.Right - $d, $rect.Y, $d, $d, 270, 90)
$path.AddArc($rect.Right - $d, $rect.Bottom - $d, $d, $d, 0, 90)
$path.AddArc($rect.X, $rect.Bottom - $d, $d, $d, 90, 90)
$path.CloseFigure()
$tile = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 0x16, 0x18, 0x1d))
$g.FillPath($tile, $path)
$edge = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 0x2a, 0x2e, 0x37)), ([float]($Size * 0.012))
$g.DrawPath($edge, $path)

# Wordmark. Measured as one string so the spacing is the font's own, then
# drawn in three runs so the punctuation can take the accent.
$accent = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 0x4f, 0x8c, 0xff))
$text = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 0xe6, 0xe9, 0xef))
$font = New-Object System.Drawing.Font $FontName, ([float]($Size * 0.26)), ([System.Drawing.FontStyle]::Bold), ([System.Drawing.GraphicsUnit]::Pixel)
$fmt = New-Object System.Drawing.StringFormat ([System.Drawing.StringFormat]::GenericTypographic)
$fmt.FormatFlags = $fmt.FormatFlags -bor [System.Drawing.StringFormatFlags]::MeasureTrailingSpaces
$runs = @(@('</', $accent), @('dex', $text), @('>', $accent))
$widths = $runs | ForEach-Object { $g.MeasureString($_[0], $font, [int]$Size, $fmt).Width }
$total = ($widths | Measure-Object -Sum).Sum
$height = $g.MeasureString('</dex>', $font, [int]$Size, $fmt).Height
$x = ($Size - $total) / 2
$y = ($Size - $height) / 2 - $Size * 0.02
for ($i = 0; $i -lt $runs.Count; $i++) {
    $g.DrawString($runs[$i][0], $font, $runs[$i][1], [float]$x, [float]$y, $fmt)
    $x += $widths[$i]
}

$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
"wrote $Out (font: $FontName)"
