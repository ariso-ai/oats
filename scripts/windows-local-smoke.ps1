param(
  [string]$Sidecar = (Join-Path (Get-Location) "src-tauri\ariso-stt-windows\target\debug\ariso-stt.exe"),
  [Parameter(Mandatory = $true)]
  [string]$Models,
  [string]$Audio,
  [int]$RepeatAudio = 1,
  [string]$Transcript,
  [int]$NotesMaxTokens = 160,
  [int]$NotesCtxSize = 2048
)

$ErrorActionPreference = "Stop"

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

function Get-WavDurationSeconds {
  param([Parameter(Mandatory = $true)][string]$Path)

  $bytes = [System.IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -lt 44) { return $null }
  $riff = [System.Text.Encoding]::ASCII.GetString($bytes, 0, 4)
  $wave = [System.Text.Encoding]::ASCII.GetString($bytes, 8, 4)
  if ($riff -ne "RIFF" -or $wave -ne "WAVE") { return $null }

  $offset = 12
  $byteRate = $null
  $dataBytes = $null
  while ($offset + 8 -le $bytes.Length) {
    $chunkId = [System.Text.Encoding]::ASCII.GetString($bytes, $offset, 4)
    $chunkSize = [BitConverter]::ToUInt32($bytes, $offset + 4)
    if ($chunkId -eq "fmt " -and $chunkSize -ge 16) {
      $byteRate = [BitConverter]::ToUInt32($bytes, $offset + 16)
    } elseif ($chunkId -eq "data") {
      $dataBytes = $chunkSize
    }
    $offset += 8 + [int]$chunkSize
    if (($chunkSize % 2) -eq 1) { $offset += 1 }
  }

  if (-not $byteRate -or -not $dataBytes) { return $null }
  [math]::Round($dataBytes / $byteRate, 3)
}

function New-RepeatedWav {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,
    [Parameter(Mandatory = $true)]
    [int]$Repeat
  )

  if ($Repeat -le 1) { return $Path }

  $bytes = [System.IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -lt 44) {
    throw "-RepeatAudio requires a WAV fixture."
  }
  $riff = [System.Text.Encoding]::ASCII.GetString($bytes, 0, 4)
  $wave = [System.Text.Encoding]::ASCII.GetString($bytes, 8, 4)
  if ($riff -ne "RIFF" -or $wave -ne "WAVE") {
    throw "-RepeatAudio requires a RIFF/WAVE fixture."
  }

  $offset = 12
  $dataOffset = $null
  $dataSize = $null
  while ($offset + 8 -le $bytes.Length) {
    $chunkId = [System.Text.Encoding]::ASCII.GetString($bytes, $offset, 4)
    $chunkSize = [BitConverter]::ToUInt32($bytes, $offset + 4)
    if ($chunkId -eq "data") {
      $dataOffset = $offset + 8
      $dataSize = [int]$chunkSize
      break
    }
    $offset += 8 + [int]$chunkSize
    if (($chunkSize % 2) -eq 1) { $offset += 1 }
  }
  if ($null -eq $dataOffset) {
    throw "WAV data chunk not found: $Path"
  }

  $outDir = Join-Path ([System.IO.Path]::GetTempPath()) "oats-windows-local-smoke"
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
  $results += [pscustomobject]@{
    step = $stt.name
    elapsedSeconds = $stt.elapsedSeconds
    audio = $effectiveAudio
    repeatAudio = $RepeatAudio
    audioSeconds = $audioSeconds
    realTimeFactor = if ($audioSeconds) { [math]::Round($stt.elapsedSeconds / $audioSeconds, 3) } else { $null }
    language = $stt.result.language
    participants = $stt.result.participants.Count
    segments = $stt.result.segments.Count
    speakers = (($stt.result.segments | ForEach-Object { $_.speaker } | Sort-Object -Unique) -join ",")
    firstSegment = if ($stt.result.segments.Count -gt 0) { $stt.result.segments[0].text } else { "" }
  }
}

if ($Transcript) {
  $ranWork = $true
  if (-not (Test-Path -LiteralPath $Transcript)) {
    throw "Transcript fixture not found: $Transcript"
  }

  $oldMaxTokens = $env:ARISO_NOTES_MAX_TOKENS
  $oldCtxSize = $env:ARISO_NOTES_CTX_SIZE
  $env:ARISO_NOTES_MAX_TOKENS = "$NotesMaxTokens"
  $env:ARISO_NOTES_CTX_SIZE = "$NotesCtxSize"
  try {
    $notes = Invoke-Timed "notes" {
      $out = & $Sidecar notes --transcript $Transcript --models $Models
      if ($LASTEXITCODE -ne 0) {
        throw "notes failed with exit code $LASTEXITCODE"
      }
      $joined = ($out -join "`n").Trim()
      if (-not $joined) {
        throw "notes produced empty output"
      }
      $joined
    }
  } finally {
    $env:ARISO_NOTES_MAX_TOKENS = $oldMaxTokens
    $env:ARISO_NOTES_CTX_SIZE = $oldCtxSize
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
