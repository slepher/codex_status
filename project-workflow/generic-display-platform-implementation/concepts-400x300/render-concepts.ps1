Add-Type -AssemblyName System.Drawing

$outDir = $PSScriptRoot
$black = [System.Drawing.Color]::Black
$white = [System.Drawing.Color]::White
$pen = [System.Drawing.Pen]::new($black, 1)
$brush = [System.Drawing.SolidBrush]::new($black)
$whiteBrush = [System.Drawing.SolidBrush]::new($white)

function New-Canvas {
    $script:bmp = [System.Drawing.Bitmap]::new(400, 300)
    $script:g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear($white)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::None
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::SingleBitPerPixelGridFit
}
function Close-Canvas($name) {
    $g.Dispose()
    $bmp.Save((Join-Path $outDir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}
function L($x1,$y1,$x2,$y2) { $g.DrawLine($pen,[int]$x1,[int]$y1,[int]$x2,[int]$y2) }
function Rect($x,$y,$w,$h) { $g.DrawRectangle($pen,[int]$x,[int]$y,[int]$w,[int]$h) }
function F($x,$y,$w,$h) { $g.FillRectangle($brush,[int]$x,[int]$y,[int]$w,[int]$h) }
function T($s,$x,$y,$size=13,$bold=$false,$right=$false,$inverse=$false) {
    $style = if ($bold) { [System.Drawing.FontStyle]::Bold } else { [System.Drawing.FontStyle]::Regular }
    $font = [System.Drawing.Font]::new('Segoe UI',[single]$size,$style,[System.Drawing.GraphicsUnit]::Pixel)
    $format = [System.Drawing.StringFormat]::new()
    $format.FormatFlags = [System.Drawing.StringFormatFlags]::NoWrap
    $format.Trimming = [System.Drawing.StringTrimming]::None
    if ($right) { $format.Alignment = [System.Drawing.StringAlignment]::Far }
    $box = [System.Drawing.RectangleF]::new([single]$x,[single]$y,[single](400-$x),[single]($size+9))
    if ($right) { $box = [System.Drawing.RectangleF]::new(0,[single]$y,[single]$x,[single]($size+9)) }
    $ink = if ($inverse) { $whiteBrush } else { $brush }
    $g.DrawString($s,$font,$ink,$box,$format)
    $format.Dispose(); $font.Dispose()
}
function Status($variant) {
    T 'CODEX' 14 8 18 $true
    T $variant 87 11 10
    T '15:55' 295 9 15 $true
    F 352 19 2 5; F 356 16 2 8; F 360 13 2 11
    Rect 369 13 18 10; F 387 16 2 4; F 372 16 11 4
    L 12 36 388 36
}
function Bar($x,$y,$w,$pct,$h=7) {
    Rect $x $y $w $h
    if ($pct -gt 0) { F ($x+1) ($y+1) ([math]::Floor(($w-2)*$pct/100)) ($h-2) }
}

# A: editorial split, emphasizing instant reading of both quotas.
New-Canvas
Status 'USAGE'
T 'Remaining' 16 50 14
T '5H' 16 81 14 $true
T '94' 15 97 72 $true
T '%' 117 145 23
T 'RESET  06:35' 18 190 12
L 199 55 199 211
T 'WEEK' 218 81 14 $true
T '25' 216 97 72 $true
T '%' 318 145 23
T 'RESET  JUN 25' 219 190 12
L 12 221 388 221
T 'PLUS' 16 235 15 $true
T 'ACCOUNT 99%' 104 238 11
T 'RC 2' 345 237 12 $true $true
L 12 269 388 269
T 'SYNC 15:55' 16 277 11
T '1 / 3' 384 277 11 $false $true
Close-Canvas 'concept-a-editorial'

# B: quiet task-list language, large right-aligned values and generous row spacing.
New-Canvas
Status 'OVERVIEW'
T 'Your usage' 17 49 24 $true
T 'PLUS  /  ACCOUNT 99%' 18 79 11
L 16 104 384 104
T '01' 18 119 11
T '5-hour limit' 56 113 18 $true
T '94%' 378 110 31 $true $true
T 'Resets at 06:35' 56 144 11
L 56 170 384 170
T '02' 18 185 11
T 'Weekly limit' 56 179 18 $true
T '25%' 378 176 31 $true $true
T 'Resets Jun 25' 56 210 11
L 16 239 384 239
T 'RESET CREDITS' 18 251 11
T '2 AVAILABLE' 379 248 15 $true $true
T 'SYNC 15:55' 18 279 10
T '1 / 3' 382 279 10 $false $true
Close-Canvas 'concept-b-list'

# C: compact instrument panel, values and horizontal progress as paired signals.
New-Canvas
Status 'LIMITS'
T '5H WINDOW' 17 52 13 $true
T '94%' 17 69 43 $true
Bar 139 90 239 94 12
T 'RESET 06:35' 140 107 12
L 16 139 384 139
T 'WEEKLY' 17 153 13 $true
T '25%' 17 170 43 $true
Bar 139 191 239 25 12
T 'RESET JUN 25' 140 208 12
L 16 239 384 239
T 'PLUS' 17 251 14 $true
T 'RC 2' 119 253 12
T 'ACCOUNT 99%' 375 253 12 $false $true
T 'SYNC 15:55' 17 279 10
T '1 / 3' 382 279 10 $false $true
Close-Canvas 'concept-c-meter'

# Weekly-only Pro variants. The absent 5H bucket takes no space or reset label.
New-Canvas
Status 'USAGE'
T 'Remaining' 16 50 14
T 'WEEK' 140 81 14 $true
T '25' 138 97 72 $true
T '%' 240 145 23
T 'RESET  JUN 25' 141 190 12
L 12 221 388 221
T 'PRO' 16 235 15 $true
T 'ACCOUNT 99%' 104 238 11
T 'RC 2' 345 237 12 $true $true
L 12 269 388 269
T 'SYNC 15:55' 16 277 11
T '1 / 3' 384 277 11 $false $true
Close-Canvas 'concept-a-pro-weekly'

New-Canvas
Status 'OVERVIEW'
T 'Your usage' 17 49 24 $true
T 'PRO  /  ACCOUNT 99%' 18 79 11
L 16 104 384 104
T '01' 18 119 11
T 'Weekly limit' 56 113 18 $true
T '25%' 378 110 31 $true $true
T 'Resets Jun 25' 56 144 11
L 56 170 384 170
L 16 239 384 239
T 'RESET CREDITS' 18 251 11
T '2 AVAILABLE' 379 248 15 $true $true
T 'SYNC 15:55' 18 279 10
T '1 / 3' 382 279 10 $false $true
Close-Canvas 'concept-b-pro-weekly'

New-Canvas
Status 'LIMITS'
T 'WEEKLY' 17 52 13 $true
T '25%' 17 69 43 $true
Bar 139 90 239 25 12
T 'RESET JUN 25' 140 107 12
L 16 139 384 139
L 16 239 384 239
T 'PRO' 17 251 14 $true
T 'RC 2' 119 253 12
T 'ACCOUNT 99%' 375 253 12 $false $true
T 'SYNC 15:55' 17 279 10
T '1 / 3' 382 279 10 $false $true
Close-Canvas 'concept-c-pro-weekly'

$pen.Dispose(); $brush.Dispose(); $whiteBrush.Dispose()
