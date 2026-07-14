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

  cargo "+$Toolchain" build `
    --manifest-path src-tauri/ariso-stt/windows/Cargo.toml `
    --release `
    --locked `
    --target $Target

  New-Item -ItemType Directory -Force src-tauri/binaries | Out-Null
  Copy-Item `
    "src-tauri/ariso-stt/windows/target/$Target/release/ariso-stt.exe" `
    "src-tauri/binaries/ariso-stt-$Target.exe" `
    -Force
} finally {
  Pop-Location
}
