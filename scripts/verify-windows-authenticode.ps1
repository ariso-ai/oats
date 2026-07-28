<#
Independently verifies the final NSIS installer and every PE it installs.

The signing audit proves Tauri routed the main executable, external sidecar,
executable resources, uninstaller, and final installer through signCommand.
Installing into a disposable runner directory proves those signed bytes made it
into the final NSIS payload.
#>
param(
  [Parameter(Mandatory = $true)]
  [string]$InstallerPath,
  [Parameter(Mandatory = $true)]
  [string]$SigningAuditPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-ValidAuthenticode {
  param([Parameter(Mandatory = $true)][string]$Path)

  $signature = Get-AuthenticodeSignature -LiteralPath $Path
  if ($signature.SignatureType -ne "Authenticode" -or $signature.Status -ne "Valid") {
    throw "Invalid Authenticode signature on '$Path': $($signature.Status)"
  }
}

function Get-SignTool {
  $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($command) {
    return $command.Source
  }

  $kitsRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
  $candidates = @(
    Get-ChildItem -LiteralPath $kitsRoot -Filter signtool.exe -File -Recurse -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -match "\\x64\\signtool\.exe$" } |
      Sort-Object FullName -Descending
  )
  if ($candidates.Count -eq 0) {
    throw "signtool.exe was not found in PATH or the Windows 10 SDK."
  }
  return $candidates[0].FullName
}

$installer = (Resolve-Path -LiteralPath $InstallerPath).Path
$audit = (Resolve-Path -LiteralPath $SigningAuditPath).Path
Assert-ValidAuthenticode -Path $installer

$signTool = Get-SignTool
& $signTool verify /pa /all /v $installer
if ($LASTEXITCODE -ne 0) {
  throw "signtool rejected the final NSIS installer."
}

$records = @(
  Get-Content -LiteralPath $audit |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
    ForEach-Object { $_ | ConvertFrom-Json }
)
if ($records.Count -eq 0) {
  throw "The Tauri signing audit is empty."
}

$requiredInvocations = [ordered]@{
  "main executable" = { param($record) $record.file -ieq "oats.exe" }
  "Windows sidecar" = { param($record) $record.file -imatch "^ariso-stt(?:-x86_64-pc-windows-msvc)?\.exe$" }
  "executable resource" = {
    param($record)
    $record.path -imatch "[\\/]binaries[\\/]llama[\\/]" -and
      [System.IO.Path]::GetExtension($record.path) -imatch "^\.(exe|dll)$"
  }
  "NSIS uninstaller" = {
    param($record)
    $record.kind -eq "nsis-uninstaller" -or $record.file -imatch "^uninstall(?:er)?\.exe$"
  }
  "final NSIS installer" = {
    param($record)
    [string]::Equals(
      [System.IO.Path]::GetFullPath($record.path),
      $installer,
      [System.StringComparison]::OrdinalIgnoreCase
    )
  }
}
foreach ($entry in $requiredInvocations.GetEnumerator()) {
  if (-not @($records | Where-Object { & $entry.Value $_ }).Count) {
    throw "Tauri signCommand audit is missing the $($entry.Key)."
  }
}

$runnerTemp = if ($env:RUNNER_TEMP) {
  [System.IO.Path]::GetFullPath($env:RUNNER_TEMP)
} else {
  [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
}
$installRoot = Join-Path $runnerTemp "oats-installed-payload-$PID"
$installRoot = [System.IO.Path]::GetFullPath($installRoot)
$runnerPrefix = $runnerTemp.TrimEnd("\") + "\"
if (-not $installRoot.StartsWith($runnerPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to install outside the disposable runner directory."
}

New-Item -ItemType Directory -Force -Path $installRoot | Out-Null
try {
  # NSIS requires /D= to be the final, unquoted portion of the raw command line,
  # including when the destination contains spaces.
  $installArguments = "/S /D=$installRoot"
  $install = Start-Process -FilePath $installer -ArgumentList $installArguments -Wait -PassThru
  if ($install.ExitCode -ne 0) {
    throw "Silent NSIS install failed with exit code $($install.ExitCode)."
  }

  $payloads = @(
    Get-ChildItem -LiteralPath $installRoot -File -Recurse |
      Where-Object { $_.Extension -in @(".exe", ".dll") }
  )
  if ($payloads.Count -eq 0) {
    throw "The NSIS installer produced no executable payloads under $installRoot."
  }
  foreach ($payload in $payloads) {
    Assert-ValidAuthenticode -Path $payload.FullName
  }

  if (-not @($payloads | Where-Object { $_.Name -ieq "oats.exe" }).Count) {
    throw "The installed payload is missing oats.exe."
  }
  if (-not @($payloads | Where-Object { $_.Name -ieq "ariso-stt.exe" }).Count) {
    throw "The installed payload is missing the Windows sidecar."
  }
  if (-not @($payloads | Where-Object { $_.Name -imatch "^uninstall(?:er)?\.exe$" }).Count) {
    throw "The installed payload is missing the signed NSIS uninstaller."
  }
  if (-not @($payloads | Where-Object {
    $_.FullName -imatch "[\\/]llama[\\/]" -and $_.Extension -in @(".exe", ".dll")
  }).Count) {
    throw "The installed payload is missing signed llama executable resources."
  }

  Write-Output "Verified Authenticode on the final installer and $($payloads.Count) installed PE payloads."
} finally {
  $uninstaller = @(
    Get-ChildItem -LiteralPath $installRoot -Filter "uninstall*.exe" -File -ErrorAction SilentlyContinue
  ) | Select-Object -First 1
  if ($uninstaller) {
    try {
      $uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -PassThru
      if ($uninstall.ExitCode -ne 0) {
        Write-Warning "Silent cleanup uninstaller exited with code $($uninstall.ExitCode)."
      }
    } catch {
      Write-Warning "Silent cleanup uninstaller could not be completed: $($_.Exception.Message)"
    }
  }
  if (Test-Path -LiteralPath $installRoot) {
    Start-Sleep -Seconds 5
    Remove-Item -LiteralPath $installRoot -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $installRoot) {
      Write-Warning "Left residual files under $installRoot; the runner is disposable."
    }
  }
}
