# Gera a imagem-base (1024x1024) dos ícones: Logo.png sobre um quadrado branco
# arredondado (o logo é escuro e transparente; assim fica visível em fundos escuros).
# Uso:
#   powershell -ExecutionPolicy Bypass -File scripts\gerar-icone.ps1 -Logo Logo.png -Saida $env:TEMP\icone.png
#   npx tauri icon $env:TEMP\icone.png   (depois apagar src-tauri\icons\android e \ios)
param([string]$Logo, [string]$Saida)
Add-Type -AssemblyName System.Drawing
$n = 1024
$bmp = New-Object System.Drawing.Bitmap $n, $n, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$g.Clear([System.Drawing.Color]::Transparent)

# quadrado branco arredondado
$m = 40; $lado = $n - 2 * $m; $r = 210; $d = 2 * $r
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddArc($m, $m, $d, $d, 180, 90)
$p.AddArc($m + $lado - $d, $m, $d, $d, 270, 90)
$p.AddArc($m + $lado - $d, $m + $lado - $d, $d, $d, 0, 90)
$p.AddArc($m, $m + $lado - $d, $d, $d, 90, 90)
$p.CloseFigure()
$g.FillPath([System.Drawing.Brushes]::White, $p)

# logo centrado
$img = [System.Drawing.Image]::FromFile($Logo)
$larg = 820
$alt = [int]($img.Height * $larg / $img.Width)
$x = [int](($n - $larg) / 2); $y = [int](($n - $alt) / 2)
$g.DrawImage($img, $x, $y, $larg, $alt)

$img.Dispose(); $g.Dispose()
$bmp.Save($Saida, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
