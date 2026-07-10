param(
  [Parameter(Mandatory = $true)]
  [string]$Models,
  [string]$SegmentationArchive,
  [string]$LlamaRuntimeArchive,
  [string]$GemmaGguf,
  [string]$StageDir = (Join-Path ([System.IO.Path]::GetTempPath()) "oats-windows-local-r2"),
  [string]$PublicBase = "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev",
  [string]$Prefix = "models/windows",
  [switch]$Upload,
  [string]$R2Endpoint = $env:R2_ENDPOINT,
  [string]$R2Bucket = $env:R2_BUCKET
)

$ErrorActionPreference = "Stop"

$SegmentationArchiveUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
$SegmentationArchiveBytes = 6958444
$SegmentationArchiveSha256 = "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488"
$LlamaRuntimeUrl = "https://github.com/ggml-org/llama.cpp/releases/download/b9940/llama-b9940-bin-win-cpu-x64.zip"
$LlamaRuntimeBytes = 18216976
$LlamaRuntimeSha256 = "d5d7248c7aacaeb0c8f15311acb0f1081874aa7a5de55843702e9e2394a05788"

function Find-FirstFile {
  param([string[]]$Candidates)
  foreach ($candidate in $Candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }
  throw "Missing required file. Checked: $($Candidates -join ', ')"
}

function Find-FirstDir {
  param([string[]]$Candidates)
  foreach ($candidate in $Candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Container) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }
  throw "Missing required directory. Checked: $($Candidates -join ', ')"
}

function Copy-RequiredFile {
  param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Destination
  )
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
  Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

function Get-Sha256 {
  param([Parameter(Mandatory = $true)][string]$Path)
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-FileDigest {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][int64]$Bytes,
    [Parameter(Mandatory = $true)][string]$Sha256
  )
  $item = Get-Item -LiteralPath $Path
  $actualSha = Get-Sha256 -Path $Path
  if ($item.Length -ne $Bytes -or $actualSha -ne $Sha256) {
    throw "Integrity check failed for $Path; expected $Bytes bytes sha256 $Sha256, got $($item.Length) bytes sha256 $actualSha"
  }
}

function Download-VerifiedFile {
  param(
    [Parameter(Mandatory = $true)][string]$Url,
    [Parameter(Mandatory = $true)][string]$Destination,
    [Parameter(Mandatory = $true)][int64]$Bytes,
    [Parameter(Mandatory = $true)][string]$Sha256
  )
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
  $part = "$Destination.part"
  if (Test-Path -LiteralPath $part) {
    Remove-Item -LiteralPath $part -Force
  }
  try {
    Invoke-WebRequest -Uri $Url -OutFile $part
  } catch {
    if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
      & curl.exe -L --fail --output $part $Url
      if ($LASTEXITCODE -ne 0) {
        throw "curl.exe failed while downloading $Url"
      }
    } else {
      throw
    }
  }
  Assert-FileDigest -Path $part -Bytes $Bytes -Sha256 $Sha256
  Move-Item -LiteralPath $part -Destination $Destination -Force
}

function Add-Artifact {
  param(
    [System.Collections.ArrayList]$Artifacts,
    [string]$Name,
    [string]$EnvVar,
    [string]$RelativePath,
    [string]$Path
  )
  $item = Get-Item -LiteralPath $Path
  [void]$Artifacts.Add([pscustomobject]@{
    name = $Name
    env = $EnvVar
    path = ($RelativePath -replace "\\", "/")
    bytes = $item.Length
    sha256 = Get-Sha256 -Path $Path
    url = (($PublicBase.TrimEnd("/") + "/" + $Prefix.Trim("/") + "/" + ($RelativePath -replace "\\", "/")))
  })
}

if (-not (Test-Path -LiteralPath $Models -PathType Container)) {
  throw "Models directory not found: $Models"
}

$modelsRoot = (Resolve-Path -LiteralPath $Models).Path
$stageRoot = Join-Path $StageDir $Prefix
if (Test-Path -LiteralPath $stageRoot) {
  Remove-Item -LiteralPath $stageRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stageRoot | Out-Null

$parakeetSource = Find-FirstDir @(
  (Join-Path $modelsRoot "windows\parakeet-tdt-0.6b-v3\v1"),
  (Join-Path $modelsRoot "windows\parakeet-tdt-0.6b-v3"),
  (Join-Path $modelsRoot "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8"),
  (Join-Path $modelsRoot "parakeet-tdt-0.6b-v3")
)
$parakeetRel = "parakeet-tdt-0.6b-v3\v1"
foreach ($file in @("encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt")) {
  Copy-RequiredFile -Source (Join-Path $parakeetSource $file) -Destination (Join-Path $stageRoot (Join-Path $parakeetRel $file))
}

$diarSource = Find-FirstDir @(
  (Join-Path $modelsRoot "windows\speaker-diarization\v1"),
  (Join-Path $modelsRoot "windows\speaker-diarization"),
  (Join-Path $modelsRoot "speaker-diarization")
)
$segArchive = Join-Path $diarSource "sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
if ($SegmentationArchive) {
  $segArchive = (Resolve-Path -LiteralPath $SegmentationArchive).Path
  Assert-FileDigest -Path $segArchive -Bytes $SegmentationArchiveBytes -Sha256 $SegmentationArchiveSha256
} elseif (-not (Test-Path -LiteralPath $segArchive -PathType Leaf)) {
  $segArchive = Join-Path $StageDir "sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
  Download-VerifiedFile -Url $SegmentationArchiveUrl -Destination $segArchive -Bytes $SegmentationArchiveBytes -Sha256 $SegmentationArchiveSha256
} else {
  Assert-FileDigest -Path $segArchive -Bytes $SegmentationArchiveBytes -Sha256 $SegmentationArchiveSha256
}
$diarRel = "speaker-diarization\v1"
Copy-RequiredFile -Source $segArchive -Destination (Join-Path $stageRoot (Join-Path $diarRel "sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"))
$embedding = Find-FirstFile @(
  (Join-Path $diarSource "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx"),
  (Join-Path $diarSource "embedding\3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx")
)
Copy-RequiredFile -Source $embedding -Destination (Join-Path $stageRoot (Join-Path $diarRel "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx"))

$gemmaSource = Find-FirstDir @(
  (Join-Path $modelsRoot "windows\gemma-3-1b-it-qat-4bit\v1"),
  (Join-Path $modelsRoot "windows\gemma-3-1b-it-qat-4bit"),
  (Join-Path $modelsRoot "gemma-3-1b-it-qat-4bit")
)
$gemmaRel = "gemma-3-1b-it-qat-4bit\v1"
if ($GemmaGguf) {
  $gemma = (Resolve-Path -LiteralPath $GemmaGguf).Path
} else {
  $gemma = Find-FirstFile @(
    (Join-Path $gemmaSource "gemma-3-1b-it-q4_0.gguf"),
    (Join-Path $gemmaSource "gemma-3-1b-it-qat-q4_0.gguf")
  )
}
Copy-RequiredFile -Source $gemma -Destination (Join-Path $stageRoot (Join-Path $gemmaRel "gemma-3-1b-it-q4_0.gguf"))
$runtimeZip = Join-Path $gemmaSource "llama-b9940-bin-win-cpu-x64.zip"
if ($LlamaRuntimeArchive) {
  $runtimeZip = (Resolve-Path -LiteralPath $LlamaRuntimeArchive).Path
  Assert-FileDigest -Path $runtimeZip -Bytes $LlamaRuntimeBytes -Sha256 $LlamaRuntimeSha256
} elseif (-not (Test-Path -LiteralPath $runtimeZip -PathType Leaf)) {
  $runtimeZip = Join-Path $StageDir "llama-b9940-bin-win-cpu-x64.zip"
  Download-VerifiedFile -Url $LlamaRuntimeUrl -Destination $runtimeZip -Bytes $LlamaRuntimeBytes -Sha256 $LlamaRuntimeSha256
} else {
  Assert-FileDigest -Path $runtimeZip -Bytes $LlamaRuntimeBytes -Sha256 $LlamaRuntimeSha256
}
Copy-RequiredFile -Source $runtimeZip -Destination (Join-Path $stageRoot (Join-Path $gemmaRel "llama-b9940-bin-win-cpu-x64.zip"))

$artifacts = [System.Collections.ArrayList]::new()
foreach ($file in @("encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt")) {
  Add-Artifact $artifacts "parakeet-$file" "" (Join-Path $parakeetRel $file) (Join-Path $stageRoot (Join-Path $parakeetRel $file))
}
Add-Artifact $artifacts "diarization-segmentation" "ARISO_WINDOWS_DIARIZATION_SEGMENTATION_URL" (Join-Path $diarRel "sherpa-onnx-pyannote-segmentation-3-0.tar.bz2") (Join-Path $stageRoot (Join-Path $diarRel "sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"))
Add-Artifact $artifacts "diarization-embedding" "ARISO_WINDOWS_DIARIZATION_EMBEDDING_URL" (Join-Path $diarRel "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx") (Join-Path $stageRoot (Join-Path $diarRel "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx"))
Add-Artifact $artifacts "gemma-gguf" "ARISO_WINDOWS_GEMMA_GGUF_URL" (Join-Path $gemmaRel "gemma-3-1b-it-q4_0.gguf") (Join-Path $stageRoot (Join-Path $gemmaRel "gemma-3-1b-it-q4_0.gguf"))
Add-Artifact $artifacts "llama-runtime" "ARISO_WINDOWS_LLAMA_RUNTIME_URL" (Join-Path $gemmaRel "llama-b9940-bin-win-cpu-x64.zip") (Join-Path $stageRoot (Join-Path $gemmaRel "llama-b9940-bin-win-cpu-x64.zip"))

$manifest = [pscustomobject]@{
  generatedAt = (Get-Date).ToUniversalTime().ToString("o")
  sourceModels = $modelsRoot
  stage = $stageRoot
  publicBase = $PublicBase
  prefix = $Prefix
  artifacts = $artifacts
}
$manifestPath = Join-Path $stageRoot "windows-local-manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
$pinsPath = Join-Path $stageRoot "windows-local-sidecar-pins.txt"
$pins = @()
$pins += "# Update src-tauri/ariso-stt-cross/src/main.rs pins if any artifact changes."
$pins += "# Generated from $manifestPath"
$pins += ""
foreach ($artifact in $artifacts) {
  if ($artifact.name -like "parakeet-*") {
    $file = Split-Path -Leaf $artifact.path
    $pins += "Parakeet $file => size $($artifact.bytes), sha256 `"$($artifact.sha256)`""
  } else {
    $pins += "$($artifact.name) => size $($artifact.bytes), sha256 `"$($artifact.sha256)`""
  }
}
$pins | Set-Content -LiteralPath $pinsPath -Encoding UTF8

if ($Upload) {
  if (-not $R2Endpoint -or -not $R2Bucket) {
    throw "Upload requires R2_ENDPOINT and R2_BUCKET."
  }
  aws s3 cp $stageRoot "s3://$R2Bucket/$Prefix/" --recursive --endpoint-url $R2Endpoint
  if ($LASTEXITCODE -ne 0) {
    throw "aws s3 cp failed."
  }
}

Write-Host "Staged Windows Local artifacts: $stageRoot"
Write-Host "Manifest: $manifestPath"
Write-Host "Sidecar pins: $pinsPath"
Write-Host ""
Write-Host "Sidecar override environment:"
Write-Host "`$env:ARISO_WINDOWS_PARAKEET_BASE_URL = `"$PublicBase/$Prefix/parakeet-tdt-0.6b-v3/v1`""
foreach ($artifact in $artifacts | Where-Object { $_.env -and $_.env -ne "ARISO_WINDOWS_PARAKEET_BASE_URL" }) {
  Write-Host "`$env:$($artifact.env) = `"$($artifact.url)`""
}
