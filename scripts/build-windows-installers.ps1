param(
  [string]$Bundles = "nsis,msi",
  [string]$Toolchain = "stable-x86_64-pc-windows-msvc",
  [string]$Target = "x86_64-pc-windows-msvc",
  [switch]$VerboseBuild
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "import-windows-build-env.ps1")

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $Root

function Assert-ChildPath {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,
    [Parameter(Mandatory = $true)]
    [string]$Parent
  )

  $fullPath = [System.IO.Path]::GetFullPath($Path)
  $fullParent = [System.IO.Path]::GetFullPath($Parent)
  if (-not $fullParent.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {
    $fullParent = "$fullParent$([System.IO.Path]::DirectorySeparatorChar)"
  }

  if (-not $fullPath.StartsWith($fullParent, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to operate on path outside the expected directory: $fullPath"
  }

  return $fullPath
}

try {
  Import-WindowsBuildEnvironment
  $env:RUSTUP_TOOLCHAIN = $Toolchain

  $bundleNames = @([regex]::Split($Bundles, "[,\s]+") |
    Where-Object { $_ } |
    ForEach-Object { $_.ToLowerInvariant() })

  foreach ($bundleName in $bundleNames) {
    if ($bundleName -notin @("nsis", "msi")) {
      throw "Unsupported Windows bundle '$bundleName'. Use 'nsis', 'msi', or 'nsis,msi'."
    }
  }

  $bundleRoot = Assert-ChildPath `
    -Path (Join-Path $Root "src-tauri\target\$Target\release\bundle") `
    -Parent $Root

  foreach ($bundleName in $bundleNames) {
    $bundlePath = Assert-ChildPath `
      -Path (Join-Path $bundleRoot $bundleName) `
      -Parent $bundleRoot

    if (Test-Path -LiteralPath $bundlePath) {
      Remove-Item -LiteralPath $bundlePath -Recurse -Force
    }
  }

  $tauriArgs = @("run", "tauri:build", "--")
  if ($VerboseBuild) {
    $tauriArgs += "--verbose"
  }
  $tauriArgs += @("--ci", "--target", $Target, "--bundles", $Bundles, "--", "--features", "prod-api")

  & npm.cmd @tauriArgs
  if ($LASTEXITCODE -ne 0) {
    throw "Tauri Windows installer build failed with exit code $LASTEXITCODE."
  }

  $artifactCount = 0
  if ($bundleNames -contains "nsis") {
    $nsis = Get-ChildItem (Join-Path $bundleRoot "nsis") -Filter *.exe -File -ErrorAction SilentlyContinue
    if ($nsis.Count -lt 1) {
      throw "Missing NSIS .exe artifact in $bundleRoot\nsis."
    }
    $artifactCount += $nsis.Count
    $nsis | ForEach-Object { "NSIS: $($_.FullName)" }
  }

  if ($bundleNames -contains "msi") {
    $msi = Get-ChildItem (Join-Path $bundleRoot "msi") -Filter *.msi -File -ErrorAction SilentlyContinue
    if ($msi.Count -lt 1) {
      throw "Missing MSI artifact in $bundleRoot\msi."
    }
    $artifactCount += $msi.Count
    $msi | ForEach-Object { "MSI: $($_.FullName)" }
  }

  if ($env:TAURI_SIGNING_PRIVATE_KEY) {
    $sigs = Get-ChildItem $bundleRoot -Recurse -Filter *.sig -File -ErrorAction SilentlyContinue
    if ($sigs.Count -lt $artifactCount) {
      throw "Expected updater signature artifacts for each Windows installer."
    }
    $sigs | ForEach-Object { "SIG: $($_.FullName)" }
  }
} finally {
  Pop-Location
}
