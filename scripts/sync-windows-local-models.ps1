param(
  [Parameter(Mandatory = $true)]
  [string]$Models,
  [string]$StageDir = (Join-Path ([System.IO.Path]::GetTempPath()) "oats-windows-models"),
  [string]$PublicBase = "https://pub-b22579d60a5b47d8835d2c4660e7bc16.r2.dev",
  [string]$Prefix = "models",
  [string]$SpeechVersion = "v1",
  [string]$NotesVersion = "v2",
  [switch]$Upload,
  [switch]$Force,
  [string]$R2Endpoint = $env:R2_ENDPOINT,
  [string]$R2Bucket = $env:R2_BUCKET
)

$ErrorActionPreference = "Stop"

function Find-FirstDirectory {
  param([Parameter(Mandatory = $true)][string[]]$Candidates)

  foreach ($candidate in $Candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Container) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }
  throw "Missing model directory. Checked: $($Candidates -join ', ')"
}

function Find-FirstFile {
  param([Parameter(Mandatory = $true)][string[]]$Candidates)

  foreach ($candidate in $Candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }
  throw "Missing model file. Checked: $($Candidates -join ', ')"
}

function Copy-BundleFile {
  param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Bundle,
    [Parameter(Mandatory = $true)][string]$RelativePath
  )

  $destination = Join-Path $Bundle $RelativePath
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
  Copy-Item -LiteralPath $Source -Destination $destination -Force
}

function Write-BundleManifest {
  param([Parameter(Mandatory = $true)][string]$Bundle)

  $root = (Resolve-Path -LiteralPath $Bundle).Path
  $entries = Get-ChildItem -LiteralPath $root -Recurse -File |
    Where-Object { $_.Name -ne "SHA256SUMS" } |
    ForEach-Object {
      $relative = $_.FullName.Substring($root.Length + 1).Replace("\", "/")
      [pscustomobject]@{
        Path = $relative
        Hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
      }
    } |
    Sort-Object Path

  if (-not $entries) {
    throw "Bundle contains no files: $Bundle"
  }

  $body = (($entries | ForEach-Object { "$($_.Hash)  $($_.Path)" }) -join "`n") + "`n"
  $manifest = Join-Path $root "SHA256SUMS"
  [System.IO.File]::WriteAllText($manifest, $body, [System.Text.UTF8Encoding]::new($false))
  (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant()
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

  [System.BitConverter]::ToString(
    [System.Security.Cryptography.SHA256]::Create().ComputeHash($Bytes)
  ).Replace('-', '').ToLowerInvariant()
}

if (-not (Test-Path -LiteralPath $Models -PathType Container)) {
  throw "Models directory not found: $Models"
}

$modelsRoot = (Resolve-Path -LiteralPath $Models).Path
$stageBase = [System.IO.Path]::GetFullPath($StageDir)
$stageRoot = [System.IO.Path]::GetFullPath((Join-Path $stageBase $Prefix))
$stagePrefix = $stageBase.TrimEnd('\') + '\'
if (-not $stageRoot.StartsWith($stagePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Stage path escapes StageDir: $stageRoot"
}
if (Test-Path -LiteralPath $stageRoot) {
  Remove-Item -LiteralPath $stageRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stageRoot | Out-Null

$parakeetSource = Find-FirstDirectory @(
  (Join-Path $modelsRoot "windows\parakeet-tdt-0.6b-v3\$SpeechVersion"),
  (Join-Path $modelsRoot "windows\parakeet-tdt-0.6b-v3\v1"),
  (Join-Path $modelsRoot "windows\parakeet-tdt-0.6b-v3"),
  (Join-Path $modelsRoot "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8"),
  (Join-Path $modelsRoot "parakeet-tdt-0.6b-v3")
)
$parakeetPath = "windows/parakeet-tdt-0.6b-v3/$SpeechVersion"
$parakeetBundle = Join-Path $stageRoot $parakeetPath
foreach ($file in @("encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt")) {
  Copy-BundleFile -Source (Join-Path $parakeetSource $file) -Bundle $parakeetBundle -RelativePath $file
}

$diarizationSource = Find-FirstDirectory @(
  (Join-Path $modelsRoot "windows\speaker-diarization\$SpeechVersion"),
  (Join-Path $modelsRoot "windows\speaker-diarization\v1"),
  (Join-Path $modelsRoot "windows\speaker-diarization"),
  (Join-Path $modelsRoot "speaker-diarization")
)
$diarizationPath = "windows/speaker-diarization/$SpeechVersion"
$diarizationBundle = Join-Path $stageRoot $diarizationPath
$segmentationName = "sherpa-onnx-pyannote-segmentation-3-0"
$segmentation = Find-FirstFile @(
  (Join-Path $diarizationSource "$segmentationName\model.int8.onnx"),
  (Join-Path $diarizationSource "$segmentationName\model.onnx"),
  (Join-Path $diarizationSource "segmentation\model.int8.onnx"),
  (Join-Path $diarizationSource "segmentation\model.onnx")
)
Copy-BundleFile -Source $segmentation -Bundle $diarizationBundle -RelativePath "$segmentationName/model.int8.onnx"
$embeddingName = "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx"
$embedding = Find-FirstFile @(
  (Join-Path $diarizationSource $embeddingName),
  (Join-Path $diarizationSource "embedding\$embeddingName")
)
Copy-BundleFile -Source $embedding -Bundle $diarizationBundle -RelativePath $embeddingName

$gemmaSource = Find-FirstDirectory @(
  (Join-Path $modelsRoot "windows\gemma-3-1b-it-qat-4bit\$NotesVersion"),
  (Join-Path $modelsRoot "windows\gemma-3-1b-it-qat-4bit\v1"),
  (Join-Path $modelsRoot "windows\gemma-3-1b-it-qat-4bit"),
  (Join-Path $modelsRoot "gemma-3-1b-it-qat-4bit")
)
$gemmaPath = "windows/gemma-3-1b-it-qat-4bit/$NotesVersion"
$gemmaBundle = Join-Path $stageRoot $gemmaPath
$gemma = Find-FirstFile @(
  (Join-Path $gemmaSource "gemma-3-1b-it-q4_0.gguf"),
  (Join-Path $gemmaSource "gemma-3-1b-it-qat-q4_0.gguf")
)
Copy-BundleFile -Source $gemma -Bundle $gemmaBundle -RelativePath "gemma-3-1b-it-q4_0.gguf"

$runtimeFiles = @(
  "llama-cli.exe",
  "llama-cli-impl.dll",
  "llama-server-impl.dll",
  "llama-common.dll",
  "llama.dll",
  "ggml.dll",
  "ggml-base.dll",
  "libomp140.x86_64.dll",
  "mtmd.dll"
)
foreach ($file in $runtimeFiles) {
  Copy-BundleFile -Source (Find-FirstFile @((Join-Path $gemmaSource $file))) -Bundle $gemmaBundle -RelativePath $file
}
$cpuBackends = @(Get-ChildItem -LiteralPath $gemmaSource -Filter "ggml-cpu-*.dll" -File)
if (-not $cpuBackends) {
  throw "No llama.cpp CPU backend DLLs found under $gemmaSource"
}
foreach ($backend in $cpuBackends) {
  Copy-BundleFile -Source $backend.FullName -Bundle $gemmaBundle -RelativePath $backend.Name
}

$bundles = @(
  [pscustomobject]@{ Path = $parakeetPath; Directory = $parakeetBundle },
  [pscustomobject]@{ Path = $diarizationPath; Directory = $diarizationBundle },
  [pscustomobject]@{ Path = $gemmaPath; Directory = $gemmaBundle }
)
foreach ($bundle in $bundles) {
  $bundle | Add-Member -NotePropertyName ManifestSha256 -NotePropertyValue (Write-BundleManifest $bundle.Directory)
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

  foreach ($bundle in $bundles) {
    $key = "$($Prefix.Trim('/'))/$($bundle.Path)"
    $existing = & aws s3 ls "s3://$R2Bucket/$key/" --endpoint-url $R2Endpoint 2>$null
    if ($LASTEXITCODE -ne 0) {
      throw "Could not inspect s3://$R2Bucket/$key/"
    }
    if ($existing -and -not $Force) {
      $publicManifest = "$($PublicBase.TrimEnd('/'))/$key/SHA256SUMS"
      $publicHash = Get-BytesSha256 (Get-PublicBytes $publicManifest)
      if ($publicHash -eq $bundle.ManifestSha256) {
        Write-Host "Already published: s3://$R2Bucket/$key/"
        continue
      }
      throw "Immutable R2 prefix already exists with different bytes: s3://$R2Bucket/$key/"
    }
    & aws s3 cp $bundle.Directory "s3://$R2Bucket/$key/" --recursive --endpoint-url $R2Endpoint
    if ($LASTEXITCODE -ne 0) {
      throw "Upload failed for s3://$R2Bucket/$key/"
    }

    $publicManifest = "$($PublicBase.TrimEnd('/'))/$key/SHA256SUMS"
    $publicHash = Get-BytesSha256 (Get-PublicBytes $publicManifest)
    if ($publicHash -ne $bundle.ManifestSha256) {
      throw "Public manifest hash mismatch for $publicManifest"
    }
  }
}

Write-Host "Windows model bundles staged under $stageRoot"
foreach ($bundle in $bundles) {
  Write-Host "$($bundle.Path) => $($bundle.ManifestSha256)"
  Write-Host "  $($PublicBase.TrimEnd('/'))/$($Prefix.Trim('/'))/$($bundle.Path)/"
}
