param([string]$OutputDirectory = "$PSScriptRoot/../artifacts/ap01-template-integration", [switch]$Digits, [switch]$Medium,
      [ValidateRange(24,72)][int]$DigitPixels = 48, [ValidateRange(24,60)][int]$MediumPixels = 30,
      [switch]$NumericOnly)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
if (!(Test-Path -LiteralPath $OutputDirectory -PathType Container)) { throw 'Precreate output directory with escalation.' }
$chars = Get-Content -LiteralPath "$PSScriptRoot/../project-workflow/ap01-template-integration/font-characters.txt" -Raw
$points = @(32..126) + @($chars.ToCharArray() | ForEach-Object { [int]$_ } | Where-Object { $_ -gt 126 }) | Sort-Object -Unique
if ($Digits -or $Medium) { $points = @(32..126) }
$size = if ($Digits) { $DigitPixels } elseif ($Medium) { $MediumPixels } else { 13 }
$family = if ($Digits -or $Medium) { 'Bahnschrift' } else { 'Microsoft YaHei UI' }
$baseline = if ($Digits) { $DigitPixels } elseif ($Medium) { $MediumPixels } else { 14 }
$font = [System.Drawing.Font]::new($family,$size,[System.Drawing.FontStyle]::Regular,[System.Drawing.GraphicsUnit]::Pixel)
$glyphs = foreach($cp in $points) {
    if ($NumericOnly -and $cp -ne 32 -and $cp -ne 37 -and $cp -ne 45 -and $cp -ne 46 -and ($cp -lt 48 -or $cp -gt 57)) {
        @{cp=$cp;advance=0;width=0;height=0;left=0;offset_y=0;bits=''}
        continue
    }
    $bitmap = [System.Drawing.Bitmap]::new(96,80)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.Clear([System.Drawing.Color]::Black)
    $graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $value = [string][char]$cp
    $advance = [Math]::Round($graphics.MeasureString($value,$font,96,[System.Drawing.StringFormat]::GenericTypographic).Width * 16)
    if ($cp -eq 32) { $advance = 64 }
    $graphics.DrawString($value,$font,[System.Drawing.Brushes]::White,0,0,[System.Drawing.StringFormat]::GenericTypographic)
    $left=96; $top=80; $right=-1; $bottom=-1
    for($y=0;$y -lt 80;$y++) { for($x=0;$x -lt 96;$x++) {
        if($bitmap.GetPixel($x,$y).R -gt 0) {
            $left=[Math]::Min($left,$x); $top=[Math]::Min($top,$y)
            $right=[Math]::Max($right,$x); $bottom=[Math]::Max($bottom,$y)
        }
    }}
    $width=[Math]::Max(0,$right-$left+1); $height=[Math]::Max(0,$bottom-$top+1)
    $bytes=[System.Collections.Generic.List[byte]]::new()
    if($width -gt 0) {
        for($y=$top;$y -le $bottom;$y++) {
            for($offset=0;$offset -lt $width;$offset+=2) {
                $hi=[int][Math]::Round($bitmap.GetPixel($left+$offset,$y).R/17)
                $lo=0
                if($offset+1 -lt $width) { $lo=[int][Math]::Round($bitmap.GetPixel($left+$offset+1,$y).R/17) }
                $bytes.Add([byte](($hi -shl 4) -bor $lo))
            }
        }
    }
    $graphics.Dispose(); $bitmap.Dispose()
    @{cp=$cp;advance=$advance;width=$width;height=$height;left=$(if($width){$left}else{0});offset_y=$(if($height){$baseline-$bottom-1}else{0});bits=[Convert]::ToBase64String($bytes.ToArray())}
}
$font.Dispose()
$stem = if ($Digits) { 'ap01-digits' } elseif ($Medium) { 'ap01-medium' } else { 'ap01-labels' }
$jsonPath=Join-Path $OutputDirectory "$stem-glyphs.json"
[System.IO.File]::WriteAllText($jsonPath,(ConvertTo-Json -InputObject @($glyphs) -Depth 4),[System.Text.UTF8Encoding]::new($false))
$slot = if ($Digits) { 'ntreg64' } elseif ($Medium) { 'nt30' } else { 'ntthin18' }
python -B "$PSScriptRoot/pack-ap01-font.py" $jsonPath (Join-Path $OutputDirectory "$stem.csfn") $slot $size
if ($LASTEXITCODE) { throw 'CSFN packing failed' }
