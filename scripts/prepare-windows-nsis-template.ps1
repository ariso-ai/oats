<#
Prepares the pinned Tauri NSIS template with the oats WiX-to-NSIS migration fix.

Tauri 2.11.4 checks $INSTDIR after successfully uninstalling a legacy WiX/MSI
package. When a separate NSIS installation also exists, $INSTDIR points at that
installation and the migration is falsely reported as an uninstall failure.
The patched check only verifies the old executable path for NSIS-to-NSIS
replacement; WiX/MSI migration relies on the uninstaller exit code.
#>
param(
  [Parameter(Mandatory = $true)]
  [string]$OutputPath,
  [string]$SourcePath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$templateUrl = "https://raw.githubusercontent.com/tauri-apps/tauri/tauri-cli-v2.11.4/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi"
$templateSha256 = "20F4ECC730DEFB71F1342EAEAEC4021DF13BE3D843ABBA0EFFE88EA5835FA079"
$expectedCliVersion = "2.11.4"
$cliPackagePath = Join-Path $PSScriptRoot "..\node_modules\@tauri-apps\cli\package.json"
$cliVersion = (Get-Content -LiteralPath $cliPackagePath -Raw | ConvertFrom-Json).version
if ($cliVersion -ne $expectedCliVersion) {
  throw "The pinned NSIS template targets Tauri CLI $expectedCliVersion, but $cliVersion is installed. Review and update this patch before building."
}

$output = [System.IO.Path]::GetFullPath($OutputPath)
$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd("\") + "\"
if (-not $output.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "The generated NSIS template must stay under the temporary directory."
}

$download = "$output.download"
try {
  if ($SourcePath) {
    $source = (Resolve-Path -LiteralPath $SourcePath).Path
    Copy-Item -LiteralPath $source -Destination $download -Force
  } else {
    Invoke-WebRequest -Uri $templateUrl -OutFile $download
  }

  $actualHash = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash
  if ($actualHash -ne $templateSha256) {
    throw "Pinned Tauri NSIS template hash mismatch: expected $templateSha256, got $actualHash."
  }

  $template = [System.IO.File]::ReadAllText($download)
  $oldCheck = @'
    ${If} $0 <> 0
    ${OrIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
'@
  $newCheck = @'
    ; A successful WiX/MSI uninstall is proven by its exit code. Only an NSIS
    ; replacement has an old executable path that this installer can verify.
    ${If} $WixMode = 0
    ${AndIf} ${FileExists} "$4\${MAINBINARYNAME}.exe"
      StrCpy $0 2
    ${EndIf}
    ${If} $0 <> 0
'@

  $matches = ([regex]::Matches($template, [regex]::Escape($oldCheck))).Count
  if ($matches -ne 1) {
    throw "Expected exactly one Tauri uninstall verification block, found $matches."
  }

  $patched = $template.Replace($oldCheck, $newCheck)
  [System.IO.File]::WriteAllText(
    $output,
    $patched,
    [System.Text.UTF8Encoding]::new($false)
  )
  Write-Output $output
} finally {
  Remove-Item -LiteralPath $download -Force -ErrorAction SilentlyContinue
}
