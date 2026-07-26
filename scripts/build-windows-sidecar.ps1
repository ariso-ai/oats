<#
Produces the target-suffixed external binary name Tauri expects when packaging
Windows. The Rust project remains the implementation source of truth; this
script does not download inference models or bundle runtime model assets.
#>
param(
  [string]$Toolchain = "stable-x86_64-pc-windows-msvc",
  [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "import-windows-build-env.ps1")

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $Root

try {
  Import-WindowsBuildEnvironment

  $output = Join-Path $Root "src-tauri\ariso-stt\windows\target\$Target\release\ariso-stt.exe"
  Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue
  & cargo "+$Toolchain" build `
    --manifest-path src-tauri/ariso-stt/windows/Cargo.toml `
    --release `
    --locked `
    --target $Target
  if ($LASTEXITCODE -ne 0) {
    throw "Windows sidecar build failed with exit code $LASTEXITCODE."
  }
  if (-not (Test-Path -LiteralPath $output -PathType Leaf)) {
    throw "Windows sidecar build did not produce $output."
  }

  New-Item -ItemType Directory -Force src-tauri/binaries | Out-Null
  Copy-Item `
    $output `
    "src-tauri/binaries/ariso-stt-$Target.exe" `
    -Force

  & (Join-Path $PSScriptRoot "prepare-windows-llama-runtime.ps1")
} finally {
  Pop-Location
}
