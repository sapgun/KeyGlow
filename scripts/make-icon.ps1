Add-Type -AssemblyName System.Drawing

$outDir = Join-Path $PSScriptRoot "..\src-tauri\icons"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$path = Join-Path $PSScriptRoot "..\app-icon.png"

$size = 1024
$bmp = New-Object System.Drawing.Bitmap $size, $size
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$g.Clear([System.Drawing.Color]::FromArgb(255, 12, 13, 16))

$glow = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(70, 182, 242, 92))
$g.FillEllipse($glow, 80, 80, 864, 864)

$bg = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 22, 25, 32))
$g.FillEllipse($bg, 170, 170, 684, 684)

$pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 182, 242, 92), 18)
$g.DrawEllipse($pen, 190, 190, 644, 644)

$font = New-Object System.Drawing.Font("Segoe UI", 380, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
$brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 230, 255, 180))
$format = New-Object System.Drawing.StringFormat
$format.Alignment = [System.Drawing.StringAlignment]::Center
$format.LineAlignment = [System.Drawing.StringAlignment]::Center
$g.DrawString("K", $font, $brush, (New-Object System.Drawing.RectangleF 0, 20, $size, $size), $format)

$bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
Write-Host "wrote $path"
