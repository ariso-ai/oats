<#
Stages the Microsoft Visual C++ x64 runtime for app-local deployment.

Release builds source the DLLs from Visual Studio's redistributable directory,
validate Microsoft's Authenticode signatures, and place identical copies beside
both Windows native entry points:

  - binaries/windows-runtime -> installed beside oats.exe and ariso-stt.exe
  - binaries/llama           -> installed beside llama-server.exe and its DLLs

The generated provenance files make the exact runtime version and hashes in an
installer auditable without committing Microsoft binaries to Git.
#>
param(
  [string]$SourceDirectory,
  [string]$RootDestination,
  [string]$LlamaDestination
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$BinariesRoot = [System.IO.Path]::GetFullPath((Join-Path $Root "src-tauri\binaries"))
$RequiredFiles = @(
  "vcruntime140.dll",
  "vcruntime140_1.dll",
  "msvcp140.dll"
)

if (-not $RootDestination) {
  $RootDestination = Join-Path $BinariesRoot "windows-runtime"
}
if (-not $LlamaDestination) {
  $LlamaDestination = Join-Path $BinariesRoot "llama"
}

function Assert-ChildPath {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Parent
  )

  $fullPath = [System.IO.Path]::GetFullPath($Path)
  $fullParent = [System.IO.Path]::GetFullPath($Parent).TrimEnd("\") + "\"
  if (-not $fullPath.StartsWith($fullParent, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "VC runtime destination must stay inside $Parent."
  }
  return $fullPath
}

function Test-RuntimeDirectory {
  param([Parameter(Mandatory = $true)][string]$Path)

  if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
    return $false
  }
  foreach ($name in $RequiredFiles) {
    if (-not (Test-Path -LiteralPath (Join-Path $Path $name) -PathType Leaf)) {
      return $false
    }
  }
  return $true
}

function Get-RuntimeVersion {
  param([Parameter(Mandatory = $true)][string]$Path)

  $version = (Get-Item -LiteralPath (Join-Path $Path "vcruntime140.dll")).VersionInfo.FileVersion
  try {
    return [version]$version
  } catch {
    throw "Invalid Visual C++ runtime version '$version' under $Path."
  }
}

function Find-VisualStudioRuntime {
  $candidates = @()

  if ($env:VCToolsRedistDir) {
    $candidates += Get-ChildItem -LiteralPath $env:VCToolsRedistDir `
      -Directory -Filter "Microsoft.VC*.CRT" -Recurse -ErrorAction SilentlyContinue
  }

  $visualStudioRoots = @(
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio"),
    (Join-Path $env:ProgramFiles "Microsoft Visual Studio")
  ) | Where-Object { Test-Path -LiteralPath $_ }
  foreach ($visualStudioRoot in $visualStudioRoots) {
    $candidates += Get-ChildItem -LiteralPath $visualStudioRoot `
      -Directory -Filter "Microsoft.VC*.CRT" -Recurse -ErrorAction SilentlyContinue
  }

  $matching = @(
    $candidates |
      Where-Object {
        $_.FullName -match "[\\/]x64[\\/]" -and
        (Test-RuntimeDirectory -Path $_.FullName)
      } |
      Sort-Object -Property @{ Expression = { Get-RuntimeVersion -Path $_.FullName } } -Descending
  )
  if ($matching.Count -eq 0) {
    throw "The Visual Studio x64 redistributable directory was not found. Install the C++ build workload or pass -SourceDirectory explicitly."
  }
  return $matching[0].FullName
}

function Get-ValidatedRuntimeFile {
  param([Parameter(Mandatory = $true)][string]$Path)

  $file = Get-Item -LiteralPath $Path
  $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
  $subject = if ($signature.SignerCertificate) {
    $signature.SignerCertificate.Subject
  } else {
    ""
  }
  if ($signature.Status -ne "Valid" -or $subject -notmatch "(^|,\s*)O=Microsoft Corporation(,|$)") {
    throw "Refusing to package untrusted VC runtime file '$Path' (signature=$($signature.Status), signer='$subject')."
  }

  $version = $file.VersionInfo.FileVersion
  if ($version -notmatch "^14\.") {
    throw "Unexpected Visual C++ runtime version '$version' for '$Path'."
  }

  return [ordered]@{
    name = $file.Name.ToLowerInvariant()
    version = $version
    size = $file.Length
    sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    signer = $subject
  }
}

$rootDestinationPath = Assert-ChildPath -Path $RootDestination -Parent $BinariesRoot
$llamaDestinationPath = Assert-ChildPath -Path $LlamaDestination -Parent $BinariesRoot
if (-not (Test-Path -LiteralPath $llamaDestinationPath -PathType Container)) {
  throw "The llama runtime must be staged before the VC runtime: $llamaDestinationPath"
}

$sourcePath = if ($SourceDirectory) {
  (Resolve-Path -LiteralPath $SourceDirectory).Path
} else {
  Find-VisualStudioRuntime
}
if (-not (Test-RuntimeDirectory -Path $sourcePath)) {
  throw "VC runtime source is missing one or more required x64 DLLs: $sourcePath"
}

$metadata = @()
foreach ($name in $RequiredFiles) {
  $metadata += Get-ValidatedRuntimeFile -Path (Join-Path $sourcePath $name)
}
$versions = @($metadata | ForEach-Object { $_.version } | Select-Object -Unique)
if ($versions.Count -ne 1) {
  throw "VC runtime files must have one consistent version; found $($versions -join ', ')."
}

if (Test-Path -LiteralPath $rootDestinationPath) {
  Remove-Item -LiteralPath $rootDestinationPath -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $rootDestinationPath | Out-Null

foreach ($name in $RequiredFiles) {
  $source = Join-Path $sourcePath $name
  Copy-Item -LiteralPath $source -Destination (Join-Path $rootDestinationPath $name) -Force
  Copy-Item -LiteralPath $source -Destination (Join-Path $llamaDestinationPath $name) -Force
}

$provenance = [ordered]@{
  schemaVersion = 1
  architecture = "x64"
  version = $versions[0]
  files = $metadata
}
$json = $provenance | ConvertTo-Json -Depth 5
$json | Set-Content -LiteralPath (Join-Path $rootDestinationPath "vc-runtime.json") -Encoding utf8
$json | Set-Content -LiteralPath (Join-Path $llamaDestinationPath "vc-runtime.json") -Encoding utf8

Write-Output "Staged Microsoft Visual C++ runtime $($versions[0]) for app-local deployment."
