<#
Verifies an installed or staged Oats Windows payload contains a trusted,
internally consistent app-local Visual C++ runtime and that both native entry
points can start with those files in their executable directories.
#>
param(
  [Parameter(Mandatory = $true)]
  [string]$RootRuntimeDirectory,
  [Parameter(Mandatory = $true)]
  [string]$LlamaRuntimeDirectory,
  [string]$SidecarPath,
  [string]$LlamaServerPath,
  [string]$TranscriptionAudioPath,
  [string]$ModelsPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RequiredFiles = @(
  "vcruntime140.dll",
  "vcruntime140_1.dll",
  "msvcp140.dll"
)

function Assert-TrustedRuntimeDirectory {
  param([Parameter(Mandatory = $true)][string]$Path)

  $directory = (Resolve-Path -LiteralPath $Path).Path
  $manifestPath = Join-Path $directory "vc-runtime.json"
  $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
  if ($manifest.schemaVersion -ne 1 -or $manifest.architecture -ne "x64") {
    throw "Invalid VC runtime provenance under '$directory'."
  }

  foreach ($name in $RequiredFiles) {
    $file = Join-Path $directory $name
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
      throw "Missing app-local VC runtime file '$file'."
    }
    $entry = @($manifest.files | Where-Object { $_.name -ieq $name })
    if ($entry.Count -ne 1) {
      throw "VC runtime provenance must contain exactly one '$name' entry."
    }
    $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $entry[0].sha256) {
      throw "VC runtime hash mismatch for '$file'."
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $file
    $subject = if ($signature.SignerCertificate) {
      $signature.SignerCertificate.Subject
    } else {
      ""
    }
    if ($signature.Status -ne "Valid" -or $subject -notmatch "(^|,\s*)O=Microsoft Corporation(,|$)") {
      throw "Invalid Microsoft signature on '$file'."
    }
  }
  return $manifest
}

function Invoke-NativeProbe {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Arguments
  )

  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $Path
  $startInfo.Arguments = $Arguments
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) {
    throw "Failed to launch native runtime probe '$Path'."
  }
  $stdout = $process.StandardOutput.ReadToEnd()
  $stderr = $process.StandardError.ReadToEnd()
  $process.WaitForExit()
  return [pscustomobject]@{
    exitCode = $process.ExitCode
    output = "$stdout`n$stderr".Trim()
  }
}

function Assert-AppLocalRuntimeLoaded {
  param([Parameter(Mandatory = $true)][string]$ServerPath)

  $expectedDirectory = [System.IO.Path]::GetFullPath(
    [System.IO.Path]::GetDirectoryName($ServerPath)
  ).TrimEnd("\")
  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $ServerPath
  $startInfo.Arguments = "--host 127.0.0.1 --port 0"
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo

  try {
    if (-not $process.Start()) {
      throw "Failed to launch llama-server for module-resolution verification."
    }
    Start-Sleep -Milliseconds 750
    $process.Refresh()
    if ($process.HasExited) {
      $stdout = $process.StandardOutput.ReadToEnd()
      $stderr = $process.StandardError.ReadToEnd()
      throw "llama-server exited before module inspection: $stdout $stderr"
    }

    $modules = @($process.Modules)
    foreach ($name in $RequiredFiles) {
      $module = @($modules | Where-Object { $_.ModuleName -ieq $name })
      if ($module.Count -ne 1) {
        throw "llama-server did not load exactly one '$name' module."
      }
      $actualDirectory = [System.IO.Path]::GetFullPath(
        [System.IO.Path]::GetDirectoryName($module[0].FileName)
      ).TrimEnd("\")
      if (-not $actualDirectory.Equals($expectedDirectory, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "llama-server loaded '$name' from '$actualDirectory' instead of its app-local directory."
      }
    }
  } finally {
    if (-not $process.HasExited) {
      $process.Kill()
      $process.WaitForExit()
    }
    $process.Dispose()
  }
}

function Assert-SidecarTranscription {
  param(
    [Parameter(Mandatory = $true)][string]$Sidecar,
    [Parameter(Mandatory = $true)][string]$Audio,
    [Parameter(Mandatory = $true)][string]$Models
  )

  $sidecarDirectory = [System.IO.Path]::GetDirectoryName($Sidecar)
  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $Sidecar
  $startInfo.Arguments = "--audio `"$Audio`" --models `"$Models`" --format json"
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo

  try {
    if (-not $process.Start()) {
      throw "Failed to launch the end-to-end transcription probe."
    }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    Start-Sleep -Milliseconds 750
    $process.Refresh()
    if (-not $process.HasExited) {
      $modules = @($process.Modules)
      $sidecarBytes = [System.Text.Encoding]::ASCII.GetString(
        [System.IO.File]::ReadAllBytes($Sidecar)
      )
      $importedRuntimeNames = @(
        [regex]::Matches($sidecarBytes, "(?i)VCRUNTIME140(?:_1)?\.dll") |
          ForEach-Object { $_.Value.ToLowerInvariant() } |
          Select-Object -Unique
      )
      foreach ($name in $importedRuntimeNames) {
        $module = @($modules | Where-Object { $_.ModuleName -ieq $name })
        if ($module.Count -ne 1) {
          $loadedRuntimeModules = @(
            $modules |
              Where-Object { $_.ModuleName -match "(?i)(vcruntime|msvcp)" } |
              ForEach-Object { "$($_.ModuleName)=$($_.FileName)" }
          )
          throw "ariso-stt did not load exactly one '$name' module during transcription. Loaded runtime modules: $($loadedRuntimeModules -join ', ')"
        }
        $actualDirectory = [System.IO.Path]::GetDirectoryName($module[0].FileName)
        if (-not $actualDirectory.Equals($sidecarDirectory, [System.StringComparison]::OrdinalIgnoreCase)) {
          throw "ariso-stt loaded '$name' from '$actualDirectory' instead of its app-local directory."
        }
      }
    }
    if (-not $process.WaitForExit(300000)) {
      throw "The end-to-end transcription probe exceeded five minutes."
    }
    $stdout = $stdoutTask.Result
    $stderr = $stderrTask.Result
    if ($process.ExitCode -ne 0) {
      throw "The end-to-end transcription probe failed: $stderr"
    }
    try {
      $payload = $stdout | ConvertFrom-Json
    } catch {
      throw "The transcription probe did not emit valid JSON: $stdout"
    }
    if (-not $payload.PSObject.Properties["text"] -and -not $payload.PSObject.Properties["segments"]) {
      throw "The transcription JSON has neither text nor segments."
    }
  } finally {
    if (-not $process.HasExited) {
      $process.Kill()
      $process.WaitForExit()
    }
    $process.Dispose()
  }
}

$rootManifest = Assert-TrustedRuntimeDirectory -Path $RootRuntimeDirectory
$llamaManifest = Assert-TrustedRuntimeDirectory -Path $LlamaRuntimeDirectory
foreach ($name in $RequiredFiles) {
  $rootHash = (Get-FileHash -LiteralPath (Join-Path $RootRuntimeDirectory $name) -Algorithm SHA256).Hash
  $llamaHash = (Get-FileHash -LiteralPath (Join-Path $LlamaRuntimeDirectory $name) -Algorithm SHA256).Hash
  if ($rootHash -ne $llamaHash) {
    throw "Root and llama copies of '$name' differ."
  }
}

if ($SidecarPath) {
  $sidecar = (Resolve-Path -LiteralPath $SidecarPath).Path
  $result = Invoke-NativeProbe -Path $sidecar -Arguments "--help"
  if ($result.exitCode -ne 0 -or $result.output -notmatch "local inference sidecar") {
    throw "The packaged Windows transcription sidecar failed its startup probe: $($result.output)"
  }
}

if ($LlamaServerPath) {
  $server = (Resolve-Path -LiteralPath $LlamaServerPath).Path
  $result = Invoke-NativeProbe -Path $server -Arguments "--version"
  if ($result.exitCode -ne 0 -or [string]::IsNullOrWhiteSpace($result.output)) {
    throw "The packaged llama server failed its startup probe: $($result.output)"
  }
  Assert-AppLocalRuntimeLoaded -ServerPath $server
}

if ([bool]$TranscriptionAudioPath -xor [bool]$ModelsPath) {
  throw "Provide both -TranscriptionAudioPath and -ModelsPath for an end-to-end transcription probe."
}
if ($TranscriptionAudioPath -and $ModelsPath) {
  if (-not $SidecarPath) {
    throw "-SidecarPath is required for an end-to-end transcription probe."
  }
  Assert-SidecarTranscription `
    -Sidecar (Resolve-Path -LiteralPath $SidecarPath).Path `
    -Audio (Resolve-Path -LiteralPath $TranscriptionAudioPath).Path `
    -Models (Resolve-Path -LiteralPath $ModelsPath).Path
}

Write-Output "Verified Microsoft VC runtime $($rootManifest.version) in both app-local directories."
