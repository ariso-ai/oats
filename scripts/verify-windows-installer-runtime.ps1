<#
Installs the NSIS package and administratively extracts the MSI into disposable
directories, then verifies that both carry the app-local Visual C++ runtime and
that the installed ariso-stt and llama-server binaries can start.
#>
param(
  [string]$NsisInstallerPath,
  [string]$MsiInstallerPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Verifier = Join-Path $PSScriptRoot "verify-windows-vc-runtime.ps1"
if (-not $NsisInstallerPath -and -not $MsiInstallerPath) {
  throw "Provide at least one Windows installer to verify."
}
$NsisInstaller = if ($NsisInstallerPath) {
  (Resolve-Path -LiteralPath $NsisInstallerPath).Path
} else {
  $null
}
$MsiInstaller = if ($MsiInstallerPath) {
  (Resolve-Path -LiteralPath $MsiInstallerPath).Path
} else {
  $null
}
$TempParent = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd("\") + "\"
$ScratchRoot = Join-Path $TempParent "oats-vc-runtime-installer-$PID"
$ScratchRoot = [System.IO.Path]::GetFullPath($ScratchRoot)
if (-not $ScratchRoot.StartsWith($TempParent, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to use a verification directory outside the system temporary directory."
}

function Invoke-Process {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Arguments,
    [Parameter(Mandatory = $true)][string]$Description
  )

  $process = Start-Process -FilePath $Path -ArgumentList $Arguments -Wait -PassThru
  if ($process.ExitCode -ne 0) {
    throw "$Description failed with exit code $($process.ExitCode)."
  }
}

function Assert-InstalledPayload {
  param([Parameter(Mandatory = $true)][string]$SearchRoot)

  $sidecars = @(
    Get-ChildItem -LiteralPath $SearchRoot -Recurse -File -Filter "ariso-stt.exe" -ErrorAction SilentlyContinue
  )
  if ($sidecars.Count -ne 1) {
    throw "Expected exactly one installed ariso-stt.exe under '$SearchRoot'; found $($sidecars.Count)."
  }
  $appRoot = $sidecars[0].Directory.FullName
  $llamaServer = Join-Path $appRoot "llama\llama-server.exe"
  if (-not (Test-Path -LiteralPath $llamaServer -PathType Leaf)) {
    throw "Installed payload is missing '$llamaServer'."
  }

  & $Verifier `
    -RootRuntimeDirectory $appRoot `
    -LlamaRuntimeDirectory (Join-Path $appRoot "llama") `
    -SidecarPath $sidecars[0].FullName `
    -LlamaServerPath $llamaServer
}

New-Item -ItemType Directory -Force -Path $ScratchRoot | Out-Null
$NsisRoot = Join-Path $ScratchRoot "nsis"
$MsiRoot = Join-Path $ScratchRoot "msi"
$NsisUninstalled = -not [bool]$NsisInstaller

try {
  if ($NsisInstaller) {
    New-Item -ItemType Directory -Force -Path $NsisRoot | Out-Null
    Invoke-Process `
      -Path $NsisInstaller `
      -Arguments "/S /D=$NsisRoot" `
      -Description "Silent NSIS installation"
    Assert-InstalledPayload -SearchRoot $NsisRoot

    $uninstaller = @(
      Get-ChildItem -LiteralPath $NsisRoot -File -Filter "uninstall*.exe" -ErrorAction SilentlyContinue
    ) | Select-Object -First 1
    if (-not $uninstaller) {
      throw "NSIS payload is missing its uninstaller."
    }
    Invoke-Process -Path $uninstaller.FullName -Arguments "/S" -Description "Silent NSIS uninstall"
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    while ((Test-Path -LiteralPath $NsisRoot) -and [DateTime]::UtcNow -lt $deadline) {
      Start-Sleep -Milliseconds 250
    }
    if (Test-Path -LiteralPath $NsisRoot) {
      throw "Silent NSIS uninstall left its installation directory behind."
    }
    $NsisUninstalled = $true
  }

  if ($MsiInstaller) {
    New-Item -ItemType Directory -Force -Path $MsiRoot | Out-Null
    $msiArguments = "/a `"$MsiInstaller`" /qn TARGETDIR=`"$MsiRoot`""
    Invoke-Process -Path "msiexec.exe" -Arguments $msiArguments -Description "MSI administrative extraction"
    Assert-InstalledPayload -SearchRoot $MsiRoot
  }

  $verifiedKinds = @()
  if ($NsisInstaller) { $verifiedKinds += "NSIS" }
  if ($MsiInstaller) { $verifiedKinds += "MSI" }
  Write-Output "Verified app-local VC runtime and native startup in $($verifiedKinds -join ' and ') payloads."
} finally {
  if (-not $NsisUninstalled) {
    $uninstaller = @(
      Get-ChildItem -LiteralPath $NsisRoot -File -Filter "uninstall*.exe" -ErrorAction SilentlyContinue
    ) | Select-Object -First 1
    if ($uninstaller) {
      try {
        Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait | Out-Null
        Start-Sleep -Seconds 2
      } catch {
        Write-Warning "Could not run the disposable NSIS uninstaller: $($_.Exception.Message)"
      }
    }
  }
  if (Test-Path -LiteralPath $ScratchRoot) {
    Remove-Item -LiteralPath $ScratchRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
