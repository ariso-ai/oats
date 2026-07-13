param(
  [string]$OutputDirectory
)

$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $OutputDirectory) {
  $OutputDirectory = Join-Path $Root "src-tauri\windows\installer"
}

$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
$logoPath = Join-Path $Root "src\assets\oats-dark.png"
if (-not (Test-Path -LiteralPath $logoPath)) {
  throw "Missing installer logo source: $logoPath"
}

Add-Type -AssemblyName System.Drawing

function New-Canvas {
  param(
    [Parameter(Mandatory = $true)]
    [int]$Width,
    [Parameter(Mandatory = $true)]
    [int]$Height
  )

  $bitmap = [System.Drawing.Bitmap]::new(
    $Width,
    $Height,
    [System.Drawing.Imaging.PixelFormat]::Format24bppRgb
  )
  $bitmap.SetResolution(96, 96)
  return $bitmap
}

function Set-HighQualityRendering {
  param(
    [Parameter(Mandatory = $true)]
    [System.Drawing.Graphics]$Graphics
  )

  $Graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
  $Graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $Graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $Graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
}

function Save-Bitmap {
  param(
    [Parameter(Mandatory = $true)]
    [System.Drawing.Bitmap]$Bitmap,
    [Parameter(Mandatory = $true)]
    [string]$Path
  )

  $Bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Bmp)
  $saved = [System.Drawing.Image]::FromFile($Path)
  try {
    if ($saved.Width -ne $Bitmap.Width -or $saved.Height -ne $Bitmap.Height) {
      throw "Unexpected bitmap dimensions for ${Path}: $($saved.Width)x$($saved.Height)"
    }
  } finally {
    $saved.Dispose()
  }
}

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

$black = [System.Drawing.ColorTranslator]::FromHtml("#111111")
$yellow = [System.Drawing.ColorTranslator]::FromHtml("#FECA0C")
$white = [System.Drawing.Color]::White
$border = [System.Drawing.ColorTranslator]::FromHtml("#E5E5E5")
$logo = [System.Drawing.Image]::FromFile($logoPath)

try {
  $dialog = New-Canvas -Width 493 -Height 312
  try {
    $graphics = [System.Drawing.Graphics]::FromImage($dialog)
    try {
      Set-HighQualityRendering -Graphics $graphics
      $graphics.Clear($white)

      $leftPanel = [System.Drawing.SolidBrush]::new($black)
      $accent = [System.Drawing.SolidBrush]::new($yellow)
      try {
        $graphics.FillRectangle($leftPanel, 0, 0, 164, 312)
        $graphics.FillRectangle($accent, 164, 0, 4, 312)
        $graphics.DrawImage($logo, 9, 71, 146, 146)

        foreach ($bar in @(
          @{ X = 55; Y = 253; Width = 7; Height = 18 },
          @{ X = 70; Y = 244; Width = 7; Height = 36 },
          @{ X = 85; Y = 236; Width = 7; Height = 52 },
          @{ X = 100; Y = 244; Width = 7; Height = 36 },
          @{ X = 115; Y = 253; Width = 7; Height = 18 }
        )) {
          $graphics.FillRectangle($accent, $bar.X, $bar.Y, $bar.Width, $bar.Height)
        }
      } finally {
        $leftPanel.Dispose()
        $accent.Dispose()
      }
    } finally {
      $graphics.Dispose()
    }

    Save-Bitmap -Bitmap $dialog -Path (Join-Path $OutputDirectory "wix-dialog.bmp")
  } finally {
    $dialog.Dispose()
  }

  $banner = New-Canvas -Width 493 -Height 58
  try {
    $graphics = [System.Drawing.Graphics]::FromImage($banner)
    try {
      Set-HighQualityRendering -Graphics $graphics
      $graphics.Clear($white)

      $brandBlock = [System.Drawing.SolidBrush]::new($black)
      $accent = [System.Drawing.SolidBrush]::new($yellow)
      $divider = [System.Drawing.Pen]::new($border, 1)
      try {
        $graphics.FillRectangle($accent, 431, 0, 4, 57)
        $graphics.FillRectangle($brandBlock, 435, 0, 58, 57)
        $graphics.DrawImage($logo, 437, 1, 54, 54)
        $graphics.DrawLine($divider, 0, 57, 493, 57)
      } finally {
        $brandBlock.Dispose()
        $accent.Dispose()
        $divider.Dispose()
      }
    } finally {
      $graphics.Dispose()
    }

    Save-Bitmap -Bitmap $banner -Path (Join-Path $OutputDirectory "wix-banner.bmp")
  } finally {
    $banner.Dispose()
  }
} finally {
  $logo.Dispose()
}

"Generated WiX installer artwork in $OutputDirectory"
