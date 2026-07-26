# Normalizes an ordinary PowerShell session into the MSVC build environment
# expected by Cargo and Tauri. It imports an existing Visual Studio toolchain
# into the current process; installation remains the caller's responsibility.
function Get-WindowsBuildEnvironmentGaps {
  $gaps = @()
  foreach ($command in @("cl.exe", "link.exe")) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
      $gaps += $command
    }
  }

  foreach ($variable in @("INCLUDE", "LIB", "VCToolsInstallDir", "WindowsSdkDir", "WindowsSDKVersion")) {
    if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($variable, "Process"))) {
      $gaps += $variable
    }
  }
  return $gaps
}

function Import-WindowsBuildEnvironment {
  if (@(Get-WindowsBuildEnvironmentGaps).Count -eq 0) {
    return
  }

  # vswhere understands current and future Visual Studio instance layouts and
  # filters out installations that do not contain the MSVC x64 toolchain.
  $vswhere = Get-Command vswhere.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
  if (-not $vswhere) {
    $bundledVswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $bundledVswhere) {
      $vswhere = $bundledVswhere
    }
  }

  $vsDevCmd = $null
  if ($vswhere) {
    $installationPath = & $vswhere `
      -latest `
      -products * `
      -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
      -property installationPath
    if ($LASTEXITCODE -eq 0 -and $installationPath) {
      $candidate = Join-Path ($installationPath | Select-Object -First 1) "Common7\Tools\VsDevCmd.bat"
      if (Test-Path -LiteralPath $candidate) {
        $vsDevCmd = $candidate
      }
    }
  }

  # Portable Build Tools images do not always include vswhere. The fallback is
  # year-agnostic and only accepts the canonical developer-shell location.
  if (-not $vsDevCmd) {
    $visualStudioRoots = @(
      (Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio"),
      (Join-Path $env:ProgramFiles "Microsoft Visual Studio")
    ) | Where-Object { Test-Path -LiteralPath $_ }
    $vsDevCmd = $visualStudioRoots |
      ForEach-Object { Get-ChildItem -LiteralPath $_ -Filter VsDevCmd.bat -File -Recurse -ErrorAction SilentlyContinue } |
      Sort-Object FullName -Descending |
      Select-Object -ExpandProperty FullName -First 1
  }

  if (-not $vsDevCmd) {
    throw "link.exe was not found and VsDevCmd.bat could not be located. Install Visual Studio Build Tools with the C++ workload."
  }

  $envLines = & cmd.exe /d /s /c "`"$vsDevCmd`" -arch=x64 -host_arch=x64 >nul && set"
  if ($LASTEXITCODE -ne 0) {
    throw "VsDevCmd.bat failed to initialize the x64 MSVC environment (exit code $LASTEXITCODE)."
  }
  foreach ($line in $envLines) {
    if ($line -match "^(.*?)=(.*)$") {
      [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
  }

  $gaps = @(Get-WindowsBuildEnvironmentGaps)
  if ($gaps.Count -gt 0) {
    throw "Visual Studio developer environment is incomplete (missing: $($gaps -join ', ')). Repair the C++ workload and Windows SDK installation."
  }
}
