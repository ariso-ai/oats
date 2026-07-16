<#
Stages the pinned llama.cpp runtime as a Windows installer resource. The model
download remains data-only; executable inference code enters the app through
the same signed installer payload as the Tauri host and ariso-stt sidecar.
#>
param(
  [string]$Destination,
  [string]$CacheDir = (Join-Path ([System.IO.Path]::GetTempPath()) "oats-windows-runtime-cache")
)

$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $Destination) {
  $Destination = Join-Path $Root "src-tauri\binaries\llama"
}
$LockPath = Join-Path $Root "src-tauri\ariso-stt\shared\windows-models.json"
$Lock = Get-Content -LiteralPath $LockPath -Raw | ConvertFrom-Json
$Runtime = $Lock.llamaRuntime

function Get-FileSha256 {
  param([Parameter(Mandatory = $true)][string]$Path)
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$destinationPath = [System.IO.Path]::GetFullPath($Destination)
$binariesRoot = [System.IO.Path]::GetFullPath((Join-Path $Root "src-tauri\binaries"))
$binariesPrefix = $binariesRoot.TrimEnd('\') + '\'
if (-not $destinationPath.StartsWith($binariesPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Runtime destination must stay inside $binariesRoot."
}

New-Item -ItemType Directory -Force -Path $CacheDir | Out-Null
$archiveName = [System.IO.Path]::GetFileName(([Uri]$Runtime.url).AbsolutePath)
$archive = Join-Path $CacheDir $archiveName
if (Test-Path -LiteralPath $archive -PathType Leaf) {
  $valid = (Get-Item -LiteralPath $archive).Length -eq [long]$Runtime.size -and
    (Get-FileSha256 $archive) -eq $Runtime.sha256
  if (-not $valid) {
    Remove-Item -LiteralPath $archive -Force
  }
}
if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
  $partial = "$archive.part"
  Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue
  Write-Host "Downloading pinned llama.cpp runtime $($Runtime.version)..."
  Invoke-WebRequest -Uri $Runtime.url -OutFile $partial -UseBasicParsing
  if ((Get-Item -LiteralPath $partial).Length -ne [long]$Runtime.size) {
    throw "llama.cpp archive size does not match the lock."
  }
  $actualHash = Get-FileSha256 $partial
  if ($actualHash -ne $Runtime.sha256) {
    throw "llama.cpp archive hash mismatch. Expected $($Runtime.sha256), got $actualHash."
  }
  Move-Item -LiteralPath $partial -Destination $archive
}

$extractRoot = Join-Path $CacheDir "expanded-$($Runtime.sha256)"
if (-not (Test-Path -LiteralPath $extractRoot -PathType Container)) {
  New-Item -ItemType Directory -Force -Path $extractRoot | Out-Null
  Expand-Archive -LiteralPath $archive -DestinationPath $extractRoot
}

if (Test-Path -LiteralPath $destinationPath) {
  Remove-Item -LiteralPath $destinationPath -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $destinationPath | Out-Null
foreach ($file in $Runtime.files) {
  $source = Join-Path $extractRoot $file
  if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    throw "Pinned llama.cpp archive is missing $file."
  }
  Copy-Item -LiteralPath $source -Destination (Join-Path $destinationPath $file)
}

$provenance = [ordered]@{
  version = $Runtime.version
  commit = $Runtime.commit
  archiveSha256 = $Runtime.sha256
}
$provenance | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destinationPath "runtime.json") -Encoding utf8
Write-Host "Staged llama.cpp $($Runtime.version) under $destinationPath"
