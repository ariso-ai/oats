<#
Builds Windows installer bundles for local/internal QA through Tauri. This
script owns local artifact cleanup and shape validation; it does not publish
releases or provide signing credentials. Callers may pass a Tauri config overlay
for an explicitly reviewed signing test.
#>
param(
  # NSIS is the consumer/updater channel. Pass -Bundles msi explicitly for
  # internal deployment testing; public releases never build both together.
  [string]$Bundles = "nsis",
  [string]$Toolchain = "stable-x86_64-pc-windows-msvc",
  [string]$Target = "x86_64-pc-windows-msvc",
  [string]$TauriConfig,
  [switch]$VerboseBuild
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "import-windows-build-env.ps1")

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $Root
$generatedConfig = $null
$generatedTemplate = $null

# Cleanup is intentionally constrained to the target bundle directory. This
# guard exists because installer builds remove stale outputs before packaging;
# it is not a general path-validation utility for arbitrary repository scripts.
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

  $effectiveConfig = if ($TauriConfig) {
    (Resolve-Path -LiteralPath $TauriConfig).Path
  } else {
    $null
  }

  $localConfig = if ($effectiveConfig) {
    Get-Content -LiteralPath $effectiveConfig -Raw | ConvertFrom-Json
  } else {
    [pscustomobject]@{}
  }
  if (-not $localConfig.PSObject.Properties["bundle"]) {
    $localConfig | Add-Member -NotePropertyName bundle -NotePropertyValue ([pscustomobject]@{})
  }
  if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    if ($localConfig.bundle.PSObject.Properties["createUpdaterArtifacts"]) {
      $localConfig.bundle.createUpdaterArtifacts = $false
    } else {
      $localConfig.bundle | Add-Member -NotePropertyName createUpdaterArtifacts -NotePropertyValue $false
    }
  }

  $bundleNames = @([regex]::Split($Bundles, "[,\s]+") |
    Where-Object { $_ } |
    ForEach-Object { $_.ToLowerInvariant() } |
    Select-Object -Unique)

  if ($bundleNames.Count -eq 0) {
    throw "At least one Windows bundle is required. Use 'nsis', 'msi', or 'nsis,msi'."
  }

  foreach ($bundleName in $bundleNames) {
    if ($bundleName -notin @("nsis", "msi")) {
      throw "Unsupported Windows bundle '$bundleName'. Use 'nsis', 'msi', or 'nsis,msi'."
    }
  }

  if ($bundleNames -contains "nsis") {
    if (-not $localConfig.bundle.PSObject.Properties["windows"]) {
      $localConfig.bundle | Add-Member -NotePropertyName windows -NotePropertyValue ([pscustomobject]@{})
    }
    if (-not $localConfig.bundle.windows.PSObject.Properties["nsis"]) {
      $localConfig.bundle.windows | Add-Member -NotePropertyName nsis -NotePropertyValue ([pscustomobject]@{})
    }

    $generatedTemplate = Join-Path ([System.IO.Path]::GetTempPath()) "oats-tauri-nsis-$PID.nsi"
    & (Join-Path $PSScriptRoot "prepare-windows-nsis-template.ps1") -OutputPath $generatedTemplate | Out-Null
    if ($localConfig.bundle.windows.nsis.PSObject.Properties["template"]) {
      $localConfig.bundle.windows.nsis.template = $generatedTemplate
    } else {
      $localConfig.bundle.windows.nsis |
        Add-Member -NotePropertyName template -NotePropertyValue $generatedTemplate
    }
  }

  $generatedConfig = Join-Path ([System.IO.Path]::GetTempPath()) "oats-tauri-local-$PID.json"
  $localConfig | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $generatedConfig -Encoding utf8
  $effectiveConfig = $generatedConfig

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
  $tauriArgs += @("--ci", "--target", $Target, "--bundles", ($bundleNames -join ","))
  if ($effectiveConfig) {
    $tauriArgs += @("--config", $effectiveConfig)
  }
  $tauriArgs += @("--", "--features", "prod-api")

  & npm.cmd @tauriArgs
  if ($LASTEXITCODE -ne 0) {
    throw "Tauri Windows installer build failed with exit code $LASTEXITCODE."
  }

  $artifacts = @()
  if ($bundleNames -contains "nsis") {
    $nsis = Get-ChildItem (Join-Path $bundleRoot "nsis") -Filter *.exe -File -ErrorAction SilentlyContinue
    if ($nsis.Count -lt 1) {
      throw "Missing NSIS .exe artifact in $bundleRoot\nsis."
    }
    $artifacts += @($nsis)
    $nsis | ForEach-Object { "NSIS: $($_.FullName)" }
  }

  if ($bundleNames -contains "msi") {
    $msi = Get-ChildItem (Join-Path $bundleRoot "msi") -Filter *.msi -File -ErrorAction SilentlyContinue
    if ($msi.Count -lt 1) {
      throw "Missing MSI artifact in $bundleRoot\msi."
    }
    $artifacts += @($msi)
    $msi | ForEach-Object { "MSI: $($_.FullName)" }
  }

  if ($env:TAURI_SIGNING_PRIVATE_KEY) {
    foreach ($artifact in $artifacts) {
      $signaturePath = "$($artifact.FullName).sig"
      if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
        throw "Missing updater signature for $($artifact.FullName)."
      }
      "SIG: $signaturePath"
    }
  }
} finally {
  if ($generatedConfig) {
    Remove-Item -LiteralPath $generatedConfig -Force -ErrorAction SilentlyContinue
  }
  if ($generatedTemplate) {
    Remove-Item -LiteralPath $generatedTemplate -Force -ErrorAction SilentlyContinue
  }
  Pop-Location
}
