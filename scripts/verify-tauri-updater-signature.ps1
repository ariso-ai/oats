<#
Verifies a Tauri updater signature with the independent official minisign CLI.

Tauri stores both its public key and detached .sig as base64-encoded minisign
files. This script decodes those files, pins the verifier download by SHA-256,
and verifies the exact final installer bytes.
#>
param(
  [Parameter(Mandatory = $true)]
  [string]$ArtifactPath,
  [string]$SignaturePath,
  [string]$TauriConfigPath = (Join-Path $PSScriptRoot "..\src-tauri\tauri.conf.json"),
  [string]$MinisignPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$artifact = (Resolve-Path -LiteralPath $ArtifactPath).Path
if (-not $SignaturePath) {
  $SignaturePath = "$artifact.sig"
}
$signature = (Resolve-Path -LiteralPath $SignaturePath).Path
$configPath = (Resolve-Path -LiteralPath $TauriConfigPath).Path
$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$encodedPublicKey = $config.plugins.updater.pubkey
if ([string]::IsNullOrWhiteSpace($encodedPublicKey)) {
  throw "Tauri updater public key is missing from $configPath."
}

$runnerTemp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$verifyRoot = Join-Path ([System.IO.Path]::GetFullPath($runnerTemp)) "oats-minisign-verify-$PID"
New-Item -ItemType Directory -Force -Path $verifyRoot | Out-Null

try {
  if (-not $MinisignPath) {
    $archive = Join-Path $verifyRoot "minisign-0.12-win64.zip"
    Invoke-WebRequest `
      -Uri "https://github.com/jedisct1/minisign/releases/download/0.12/minisign-0.12-win64.zip" `
      -OutFile $archive `
      -MaximumRetryCount 3 `
      -RetryIntervalSec 5 `
      -TimeoutSec 120 `
      -UseBasicParsing
    $expectedHash = "37b600344e20c19314b2e82813db2bfdcc408b77b876f7727889dbd46d539479"
    $actualHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
      throw "Official minisign archive hash mismatch. Expected $expectedHash, got $actualHash."
    }
    $expanded = Join-Path $verifyRoot "minisign"
    Expand-Archive -LiteralPath $archive -DestinationPath $expanded
    $MinisignPath = Join-Path $expanded "minisign-win64\x86_64\minisign.exe"
  }
  $minisign = (Resolve-Path -LiteralPath $MinisignPath).Path

  $publicKeyFile = Join-Path $verifyRoot "updater.pub"
  $signatureFile = Join-Path $verifyRoot "artifact.minisig"
  try {
    [System.IO.File]::WriteAllBytes(
      $publicKeyFile,
      [Convert]::FromBase64String($encodedPublicKey.Trim())
    )
    [System.IO.File]::WriteAllBytes(
      $signatureFile,
      [Convert]::FromBase64String((Get-Content -LiteralPath $signature -Raw).Trim())
    )
  } catch {
    throw "Tauri updater public key or signature is not valid base64."
  }

  & $minisign -Vm $artifact -x $signatureFile -p $publicKeyFile
  if ($LASTEXITCODE -ne 0) {
    throw "Independent minisign verification rejected $artifact."
  }
  Write-Output "Verified the Tauri updater signature on $([System.IO.Path]::GetFileName($artifact))."
} finally {
  $verifyRoot = [System.IO.Path]::GetFullPath($verifyRoot)
  $runnerPrefix = [System.IO.Path]::GetFullPath($runnerTemp).TrimEnd("\") + "\"
  if (
    $verifyRoot.StartsWith($runnerPrefix, [System.StringComparison]::OrdinalIgnoreCase) -and
    (Test-Path -LiteralPath $verifyRoot)
  ) {
    Remove-Item -LiteralPath $verifyRoot -Recurse -Force
  }
}
