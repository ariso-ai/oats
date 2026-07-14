<#
Exercises the real Windows sidecar and installed local models while reporting
machine context and elapsed time. This is a developer performance/contract
probe, not the installed-app smoke test: it does not download models, drive the
Tauri UI, or validate installer and updater behavior.
#>
param(
  [string]$Sidecar,
  [Parameter(Mandatory = $true)]
  [string]$Models,
  [string]$Audio,
  [int]$RepeatAudio = 1,
  [string]$Transcript
)

$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if (-not $Sidecar) {
  $Sidecar = Join-Path $Root "src-tauri\binaries\ariso-stt-x86_64-pc-windows-msvc.exe"
}

# Wraps a sidecar operation in report-friendly timing without deciding whether
# its result is semantically correct. Each caller performs command-specific
# validation before the measurement is added to the final JSON report.
function Invoke-Timed {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Name,
    [Parameter(Mandatory = $true)]
    [scriptblock]$Body
  )

  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $result = & $Body
  $sw.Stop()
  [pscustomobject]@{
    name = $Name
    elapsedSeconds = [math]::Round($sw.Elapsed.TotalSeconds, 3)
    result = $result
  }
}

# Captures enough hardware identity to compare CPU-oriented runs without adding
# vendor-specific benchmark dependencies. Registry fallback keeps the report
# useful on machines where CIM video enumeration is restricted.
function Get-HardwareSummary {
  $cpu = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Name
  $gpus = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
    Select-Object -ExpandProperty Name
  if (-not $cpu -and $env:PROCESSOR_IDENTIFIER) {
    $cpu = $env:PROCESSOR_IDENTIFIER
  }
  if (-not $gpus) {
    $gpus = Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\Video\*\*\*" -ErrorAction SilentlyContinue |
      Where-Object { $_.DriverDesc } |
      Select-Object -ExpandProperty DriverDesc -Unique
  }
  [pscustomobject]@{
    cpu = if ($cpu) { $cpu.Trim() } else { "unknown" }
    gpu = if ($gpus) { ($gpus | ForEach-Object { $_.Trim() }) -join "; " } else { "unknown" }
  }
}

# Locates the format and data chunks once for both timing and fixture expansion.
# This deliberately remains a lightweight benchmark parser, not an alternative
# to the application's codec-aware audio decoder.
function Get-WavLayout {
  param([Parameter(Mandatory = $true)][string]$Path)

  $bytes = [System.IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -lt 12) { return $null }
  $riff = [System.Text.Encoding]::ASCII.GetString($bytes, 0, 4)
  $wave = [System.Text.Encoding]::ASCII.GetString($bytes, 8, 4)
  if ($riff -ne "RIFF" -or $wave -ne "WAVE") { return $null }

  $offset = 12
  $byteRate = $null
  $dataOffset = $null
  $dataSize = $null
  while ($offset + 8 -le $bytes.Length) {
    $chunkId = [System.Text.Encoding]::ASCII.GetString($bytes, $offset, 4)
    $chunkSize = [int64][BitConverter]::ToUInt32($bytes, $offset + 4)
    $payloadOffset = $offset + 8
    if ($payloadOffset + $chunkSize -gt $bytes.Length) { return $null }
    if ($chunkId -eq "fmt " -and $chunkSize -ge 16) {
      $byteRate = [BitConverter]::ToUInt32($bytes, $offset + 16)
    } elseif ($chunkId -eq "data" -and $null -eq $dataOffset) {
      $dataOffset = $payloadOffset
      $dataSize = [int]$chunkSize
    }
    $offset = $payloadOffset + [int]$chunkSize
    if (($chunkSize % 2) -eq 1) { $offset += 1 }
  }

  if ($null -eq $dataOffset) { return $null }
  [pscustomobject]@{
    Bytes = $bytes
    ByteRate = $byteRate
    DataOffset = $dataOffset
    DataSize = $dataSize
  }
}

# Reads RIFF metadata solely to calculate real-time factor and returns null for
# formats or layouts the lightweight probe cannot understand.
function Get-WavDurationSeconds {
  param([Parameter(Mandatory = $true)][string]$Path)

  $layout = Get-WavLayout -Path $Path
  if (-not $layout -or -not $layout.ByteRate) { return $null }
  [math]::Round($layout.DataSize / $layout.ByteRate, 3)
}

# Creates a longer deterministic fixture by repeating PCM payload bytes while
# preserving the original WAV format header. This measures scaling behavior; it
# does not simulate conversational variation or diarization quality.
function New-RepeatedWav {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,
    [Parameter(Mandatory = $true)]
    [int]$Repeat
  )

  if ($Repeat -le 1) { return $Path }

  $layout = Get-WavLayout -Path $Path
  if (-not $layout) {
    throw "-RepeatAudio requires a RIFF/WAVE fixture."
  }
  $bytes = $layout.Bytes
  $dataOffset = $layout.DataOffset
  $dataSize = $layout.DataSize

  $outDir = Join-Path ([System.IO.Path]::GetTempPath()) "oats-windows-local-benchmark"
  New-Item -ItemType Directory -Force -Path $outDir | Out-Null
  $baseName = [System.IO.Path]::GetFileNameWithoutExtension($Path)
  $outPath = Join-Path $outDir "$baseName-repeat-$Repeat.wav"

  $header = New-Object byte[] $dataOffset
  [Array]::Copy($bytes, 0, $header, 0, $dataOffset)
  $newDataSize = $dataSize * $Repeat
  [Array]::Copy([BitConverter]::GetBytes([uint32](($dataOffset - 8) + $newDataSize)), 0, $header, 4, 4)
  [Array]::Copy([BitConverter]::GetBytes([uint32]$newDataSize), 0, $header, ($dataOffset - 4), 4)

  $out = [System.IO.File]::Create($outPath)
  try {
    $out.Write($header, 0, $header.Length)
    for ($i = 0; $i -lt $Repeat; $i++) {
      $out.Write($bytes, $dataOffset, $dataSize)
    }
  } finally {
    $out.Dispose()
  }

  $outPath
}

if (-not (Test-Path -LiteralPath $Sidecar)) {
  throw "Sidecar not found: $Sidecar"
}

if (-not (Test-Path -LiteralPath $Models)) {
  throw "Models directory not found: $Models"
}

$ranWork = $false
$results = @()
$hardware = Get-HardwareSummary
$results += [pscustomobject]@{
  step = "hardware"
  cpu = $hardware.cpu
  gpu = $hardware.gpu
  runtimeBackend = "CPU by default; dedicated GPU not required"
}

# Transcription validation focuses on the shared JSON contract and timing. Text
# quality remains a manual/model-evaluation concern outside this benchmark.
if ($Audio) {
  $ranWork = $true
  if (-not (Test-Path -LiteralPath $Audio)) {
    throw "Audio fixture not found: $Audio"
  }
  $effectiveAudio = New-RepeatedWav -Path $Audio -Repeat $RepeatAudio

  $stt = Invoke-Timed "transcribe" {
    $json = & $Sidecar --audio $effectiveAudio --models $Models --format json
    if ($LASTEXITCODE -ne 0) {
      throw "transcribe failed with exit code $LASTEXITCODE"
    }
    $json | ConvertFrom-Json
  }
  $audioSeconds = Get-WavDurationSeconds -Path $effectiveAudio
  if ($stt.result.segments.Count -lt 1) {
    throw "transcription returned no segments"
  }
  $results += [pscustomobject]@{
    step = $stt.name
    elapsedSeconds = $stt.elapsedSeconds
    audio = $effectiveAudio
    repeatAudio = $RepeatAudio
    audioSeconds = $audioSeconds
    realTimeFactor = if ($audioSeconds) { [math]::Round($stt.elapsedSeconds / $audioSeconds, 3) } else { $null }
    language = $stt.result.language
    segments = $stt.result.segments.Count
    rawSpeakerKeys = (($stt.result.segments | ForEach-Object { $_.speaker } | Sort-Object -Unique) -join ",")
    firstSegment = if ($stt.result.segments.Count -gt 0) { $stt.result.segments[0].text } else { "" }
  }
}

# Notes validation exercises the same fixed generation policy as the app.
if ($Transcript) {
  $ranWork = $true
  if (-not (Test-Path -LiteralPath $Transcript)) {
    throw "Transcript fixture not found: $Transcript"
  }

  $notes = Invoke-Timed "notes" {
    $out = & $Sidecar notes --transcript $Transcript --models $Models
    if ($LASTEXITCODE -ne 0) {
      throw "notes failed with exit code $LASTEXITCODE"
    }
    $joined = ($out -join "`n").Trim()
    if (-not $joined) {
      throw "notes produced empty output"
    }
    if (-not $joined.TrimStart().StartsWith("## ")) {
      throw "notes output does not begin with a Markdown heading"
    }
    $joined
  }

  $results += [pscustomobject]@{
    step = $notes.name
    elapsedSeconds = $notes.elapsedSeconds
    startsWithHeading = ($notes.result.TrimStart().StartsWith("## "))
    preview = (($notes.result -split "`r?`n" | Select-Object -First 8) -join "`n")
  }
}

if (-not $ranWork) {
  throw "Pass -Audio, -Transcript, or both."
}

$results | ConvertTo-Json -Depth 6
