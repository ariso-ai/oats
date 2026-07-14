# Normalizes an ordinary PowerShell session into the MSVC build environment
# expected by Cargo and Tauri. It imports an existing Visual Studio toolchain
# into the current process; installation remains the caller's responsibility.
function Import-WindowsBuildEnvironment {
  if (Get-Command link.exe -ErrorAction SilentlyContinue) {
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
  foreach ($line in $envLines) {
    if ($line -match "^(.*?)=(.*)$") {
      [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
  }

  if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    throw "Visual Studio developer environment loaded, but link.exe is still unavailable. Repair the C++ build tools installation."
  }
}
