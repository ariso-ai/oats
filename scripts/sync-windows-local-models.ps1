<#
Builds the Windows model bundles from the exact upstream artifacts pinned in
`src-tauri/ariso-stt/shared/windows-models.json`. No installed oats model tree
is consulted, so a release cannot inherit stale files from its build machine.
#>
param(
  [string]$StageDir = (Join-Path ([System.IO.Path]::GetTempPath()) "oats-windows-models"),
  [string]$CacheDir = (Join-Path ([System.IO.Path]::GetTempPath()) "oats-windows-model-cache"),
  [switch]$Upload,
  [string]$R2Endpoint = $env:R2_ENDPOINT,
  [string]$R2Bucket = $env:R2_BUCKET
)

$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$LockPath = Join-Path $Root "src-tauri\ariso-stt\shared\windows-models.json"
$Lock = Get-Content -LiteralPath $LockPath -Raw | ConvertFrom-Json
if ($Lock.schemaVersion -ne 1) {
  throw "Unsupported Windows model lock schema: $($Lock.schemaVersion)"
}

function Get-LockSource {
  param([Parameter(Mandatory = $true)][string]$Name)

  $property = $Lock.sources.PSObject.Properties[$Name]
  if (-not $property) {
    throw "Windows model lock has no source named '$Name'."
  }
  $property.Value
}

function Get-LockBundle {
  param(
    [Parameter(Mandatory = $true)]$Bundles,
    [Parameter(Mandatory = $true)][string]$Id
  )

  $matches = @($Bundles | Where-Object { $_.id -eq $Id })
  if ($matches.Count -ne 1) {
    throw "Windows model lock must contain exactly one '$Id' bundle."
  }
  $matches[0]
}

function Get-FileSha256 {
  param([Parameter(Mandatory = $true)][string]$Path)

  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-PinnedFile {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)]$Source
  )

  $actualSize = (Get-Item -LiteralPath $Path).Length
  if ($actualSize -ne [long]$Source.size) {
    throw "Size mismatch for $Path. Expected $($Source.size), got $actualSize."
  }
  $actualHash = Get-FileSha256 $Path
  if ($actualHash -ne $Source.sha256) {
    throw "SHA-256 mismatch for $Path. Expected $($Source.sha256), got $actualHash."
  }
}

function Get-PinnedArtifact {
  param([Parameter(Mandatory = $true)][string]$Name)

  $source = Get-LockSource $Name
  $fileName = [System.IO.Path]::GetFileName(([Uri]$source.url).AbsolutePath)
  $sourceDir = Join-Path $CacheDir $Name
  $destination = Join-Path $sourceDir $fileName
  New-Item -ItemType Directory -Force -Path $sourceDir | Out-Null

  if (Test-Path -LiteralPath $destination -PathType Leaf) {
    try {
      Assert-PinnedFile -Path $destination -Source $source
      return $destination
    } catch {
      Remove-Item -LiteralPath $destination -Force
    }
  }

  $partial = "$destination.part"
  Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue
  Write-Host "Downloading pinned $Name artifact..."
  if (-not (Get-Command curl.exe -ErrorAction SilentlyContinue)) {
    throw "Downloading pinned model artifacts requires curl.exe."
  }
  & curl.exe `
    --fail `
    --location `
    --retry 3 `
    --retry-all-errors `
    --connect-timeout 30 `
    --output $partial `
    $source.url
  if ($LASTEXITCODE -ne 0) {
    throw "Download failed for pinned $Name artifact."
  }
  Assert-PinnedFile -Path $partial -Source $source
  Move-Item -LiteralPath $partial -Destination $destination
  $destination
}

function Expand-PinnedArchive {
  param([Parameter(Mandatory = $true)][string]$Name)

  $source = Get-LockSource $Name
  if (-not $source.archiveRoot) {
    throw "Source '$Name' is not an archive."
  }
  $archive = Get-PinnedArtifact $Name
  $extractRoot = Join-Path (Join-Path $CacheDir "expanded") "$Name-$($source.sha256)"
  $modelRoot = Join-Path $extractRoot $source.archiveRoot
  if (Test-Path -LiteralPath $modelRoot -PathType Container) {
    return $modelRoot
  }

  if (Test-Path -LiteralPath $extractRoot) {
    Remove-Item -LiteralPath $extractRoot -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $extractRoot | Out-Null
  & tar.exe -xf $archive -C $extractRoot
  if ($LASTEXITCODE -ne 0) {
    throw "Could not extract pinned $Name artifact."
  }
  if (-not (Test-Path -LiteralPath $modelRoot -PathType Container)) {
    throw "Pinned $Name archive did not contain $($source.archiveRoot)."
  }
  $modelRoot
}

function Copy-BundleFile {
  param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Bundle,
    [Parameter(Mandatory = $true)][string]$RelativePath
  )

  if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
    throw "Missing pinned source file: $Source"
  }
  $destination = Join-Path $Bundle $RelativePath
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
  Copy-Item -LiteralPath $Source -Destination $destination
}

function Write-BundleManifest {
  param([Parameter(Mandatory = $true)][string]$Bundle)

  $root = (Resolve-Path -LiteralPath $Bundle).Path
  $entries = Get-ChildItem -LiteralPath $root -Recurse -File |
    Where-Object { $_.Name -ne "SHA256SUMS" } |
    ForEach-Object {
      $relative = $_.FullName.Substring($root.Length + 1).Replace("\", "/")
      [pscustomobject]@{ Path = $relative; Hash = Get-FileSha256 $_.FullName }
    } |
    Sort-Object Path
  if (-not $entries) {
    throw "Bundle contains no files: $Bundle"
  }

  $body = (($entries | ForEach-Object { "$($_.Hash)  $($_.Path)" }) -join "`n") + "`n"
  $manifest = Join-Path $root "SHA256SUMS"
  [System.IO.File]::WriteAllText($manifest, $body, [System.Text.UTF8Encoding]::new($false))
  Get-FileSha256 $manifest
}

function New-Bundle {
  param([Parameter(Mandatory = $true)]$Definition)

  $relativePath = "$($Definition.folder)/$($Definition.prefix)"
  $directory = Join-Path $StageDir $relativePath
  New-Item -ItemType Directory -Force -Path $directory | Out-Null
  [pscustomobject]@{
    Definition = $Definition
    Path = $relativePath
    Directory = $directory
  }
}

function Get-PublicBytes {
  param([Parameter(Mandatory = $true)][string]$Url)

  for ($attempt = 1; $attempt -le 5; $attempt++) {
    try {
      return (New-Object System.Net.WebClient).DownloadData($Url)
    } catch {
      if ($attempt -eq 5) { throw }
      Start-Sleep -Seconds 2
    }
  }
}

function Get-BytesSha256 {
  param([Parameter(Mandatory = $true)][byte[]]$Bytes)

  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    [System.BitConverter]::ToString($sha.ComputeHash($Bytes)).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

$stageRoot = [System.IO.Path]::GetFullPath($StageDir)
$stageLeaf = $stageRoot.TrimEnd('\')
$driveRoot = [System.IO.Path]::GetPathRoot($stageRoot).TrimEnd('\')
$protectedRoots = @(
  $driveRoot,
  $Root.TrimEnd('\'),
  [System.IO.Path]::GetFullPath($HOME).TrimEnd('\')
)
if ($protectedRoots -contains $stageLeaf) {
  throw "Refusing to use protected directory as model staging root: $stageRoot"
}
if (Test-Path -LiteralPath $stageRoot) {
  Remove-Item -LiteralPath $stageRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stageRoot | Out-Null

$parakeet = New-Bundle (Get-LockBundle $Lock.speech "parakeet")
$parakeetSource = Expand-PinnedArchive "parakeet"
foreach ($file in $parakeet.Definition.files) {
  Copy-BundleFile -Source (Join-Path $parakeetSource $file) -Bundle $parakeet.Directory -RelativePath $file
}

$diarization = New-Bundle (Get-LockBundle $Lock.speech "diarization")
$segmentationSource = Expand-PinnedArchive "segmentation"
$embeddingSource = Get-PinnedArtifact "embedding"
Copy-BundleFile `
  -Source (Join-Path $segmentationSource "model.int8.onnx") `
  -Bundle $diarization.Directory `
  -RelativePath "sherpa-onnx-pyannote-segmentation-3-0/model.int8.onnx"
Copy-BundleFile `
  -Source $embeddingSource `
  -Bundle $diarization.Directory `
  -RelativePath "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx"

$notes = New-Bundle (Get-LockBundle $Lock.notes "gemma")
$gemmaSource = Get-PinnedArtifact "gemma"
Copy-BundleFile -Source $gemmaSource -Bundle $notes.Directory -RelativePath $notes.Definition.files[0]

$bundles = @($parakeet, $diarization, $notes)
foreach ($bundle in $bundles) {
  $manifestHash = Write-BundleManifest $bundle.Directory
  if ($manifestHash -ne $bundle.Definition.manifestSha256) {
    throw "Bundle $($bundle.Path) produced $manifestHash, but the lock pins $($bundle.Definition.manifestSha256)."
  }
  $bundle | Add-Member -NotePropertyName ManifestSha256 -NotePropertyValue $manifestHash
}

if ($Upload) {
  if (-not $R2Endpoint -or -not $R2Bucket) {
    throw "Upload requires R2_ENDPOINT and R2_BUCKET."
  }
  if ($R2Bucket.Contains('.')) {
    throw "R2_BUCKET must be a bucket name, not a public domain."
  }
  if (-not (Get-Command aws -ErrorAction SilentlyContinue)) {
    throw "Upload requires the AWS CLI."
  }

  $prefix = ([Uri]$Lock.cdnBase).AbsolutePath.Trim('/')
  foreach ($bundle in $bundles) {
    $key = "$prefix/$($bundle.Path)"
    $existing = & aws s3 ls "s3://$R2Bucket/$key/" --endpoint-url $R2Endpoint 2>$null
    if ($LASTEXITCODE -ne 0) {
      throw "Could not inspect s3://$R2Bucket/$key/."
    }
    if ($existing) {
      $publicManifest = "$($Lock.cdnBase.TrimEnd('/'))/$($bundle.Path)/SHA256SUMS"
      $publicHash = Get-BytesSha256 (Get-PublicBytes $publicManifest)
      if ($publicHash -eq $bundle.ManifestSha256) {
        Write-Host "Already published: s3://$R2Bucket/$key/"
        continue
      }
      throw "Immutable R2 prefix already contains different bytes: s3://$R2Bucket/$key/"
    }

    & aws s3 cp $bundle.Directory "s3://$R2Bucket/$key/" --recursive --endpoint-url $R2Endpoint
    if ($LASTEXITCODE -ne 0) {
      throw "Upload failed for s3://$R2Bucket/$key/."
    }
    $publicManifest = "$($Lock.cdnBase.TrimEnd('/'))/$($bundle.Path)/SHA256SUMS"
    $publicHash = Get-BytesSha256 (Get-PublicBytes $publicManifest)
    if ($publicHash -ne $bundle.ManifestSha256) {
      throw "Public manifest hash mismatch for $publicManifest."
    }
  }
}

Write-Host "Windows model bundles staged under $stageRoot"
foreach ($bundle in $bundles) {
  Write-Host "$($bundle.Path) => $($bundle.ManifestSha256)"
  Write-Host "  $($Lock.cdnBase.TrimEnd('/'))/$($bundle.Path)/"
}
