# Gera a imagem-base (1024x1024) dos ícones: Logo.png centrado, sem fundo
# (transparente). -ComFundo põe-no sobre um quadrado branco arredondado
# (útil se o ícone tiver de se ver em fundos escuros).
# Uso:
#   powershell -ExecutionPolicy Bypass -File scripts\gerar-icone.ps1 -Logo Logo.png -Saida $env:TEMP\icone.png
#   npx tauri icon $env:TEMP\icone.png   (depois apagar src-tauri\icons\android e \ios)
#   Ícone da janela em modo escuro (logo branco, 256x256):
#   powershell -ExecutionPolicy Bypass -File scripts\gerar-icone.ps1 -Logo Logo.png -Saida src-tauri\icons\icone-escuro.png -Branco -Lado 256
param([string]$Logo, [string]$Saida, [switch]$ComFundo, [switch]$Branco, [int]$Lado = 1024)
Add-Type -AssemblyName System.Drawing
$n = $Lado
$bmp = New-Object System.Drawing.Bitmap $n, $n, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$g.Clear([System.Drawing.Color]::Transparent)

if ($ComFundo) {
    # quadrado branco arredondado
    $m = 40; $lado = $n - 2 * $m; $r = 210; $d = 2 * $r
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $p.AddArc($m, $m, $d, $d, 180, 90)
    $p.AddArc($m + $lado - $d, $m, $d, $d, 270, 90)
    $p.AddArc($m + $lado - $d, $m + $lado - $d, $d, $d, 0, 90)
    $p.AddArc($m, $m + $lado - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    $g.FillPath([System.Drawing.Brushes]::White, $p)
}

# logo centrado (sem fundo ocupa quase toda a largura)
$img = [System.Drawing.Image]::FromFile($Logo)
$larg = [int]($n * $(if ($ComFundo) { 820 } else { 990 }) / 1024)
$alt = [int]($img.Height * $larg / $img.Width)
$x = [int](($n - $larg) / 2); $y = [int](($n - $alt) / 2)
$attr = New-Object System.Drawing.Imaging.ImageAttributes
if ($Branco) {
    # RGB = 1, mantém o alfa: o logo fica branco
    $cm = New-Object System.Drawing.Imaging.ColorMatrix
    $cm.Matrix00 = 0; $cm.Matrix11 = 0; $cm.Matrix22 = 0
    $cm.Matrix40 = 1; $cm.Matrix41 = 1; $cm.Matrix42 = 1
    $attr.SetColorMatrix($cm)
}
$g.DrawImage($img, (New-Object System.Drawing.Rectangle $x, $y, $larg, $alt), 0, 0, $img.Width, $img.Height, [System.Drawing.GraphicsUnit]::Pixel, $attr)

$img.Dispose(); $g.Dispose()
$bmp.Save($Saida, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
