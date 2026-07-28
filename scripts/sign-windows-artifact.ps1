<#
Authenticode-signs one PE artifact with SSL.com eSigner.

Tauri invokes this script through bundle.windows.signCommand for every Windows
binary it packages. Provider output is intentionally suppressed because it can
contain credential-bearing command lines; only a secret-free audit record is
written after the resulting signature validates.
#>
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$FilePath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$requiredEnvironment = @(
  "CODE_SIGN_TOOL_PATH",
  "ES_USERNAME",
  "ES_PASSWORD",
  "ES_CREDENTIAL_ID",
  "ES_TOTP_SECRET"
)
foreach ($name in $requiredEnvironment) {
  if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
    throw "Missing required eSigner environment variable: $name"
  }
}

$artifact = (Resolve-Path -LiteralPath $FilePath).Path
$extension = [System.IO.Path]::GetExtension($artifact).ToLowerInvariant()
if ($extension -notin @(".exe", ".dll", ".msi")) {
  throw "Refusing to Authenticode-sign unsupported artifact type '$extension'."
}

$toolRoot = (Resolve-Path -LiteralPath $env:CODE_SIGN_TOOL_PATH).Path
$jarDirectory = Join-Path $toolRoot "jar"
$jars = @(Get-ChildItem -LiteralPath $jarDirectory -Filter "code_sign_tool-*.jar" -File)
if ($jars.Count -ne 1) {
  throw "Expected exactly one SSL.com CodeSignTool jar under $jarDirectory, found $($jars.Count)."
}

$java = $null
if ($env:JAVA_HOME) {
  $candidate = Join-Path $env:JAVA_HOME "bin\java.exe"
  if (Test-Path -LiteralPath $candidate -PathType Leaf) {
    $java = $candidate
  }
}
if (-not $java) {
  $javaCommand = Get-Command java.exe -ErrorAction SilentlyContinue
  if ($javaCommand) {
    $java = $javaCommand.Source
  }
}
if (-not $java) {
  throw "Java 11 or newer is required by SSL.com CodeSignTool."
}

$arguments = @(
  "-Xmx1024M",
  "-jar",
  $jars[0].FullName,
  "sign",
  "-username=$env:ES_USERNAME",
  "-password=$env:ES_PASSWORD",
  "-credential_id=$env:ES_CREDENTIAL_ID",
  "-totp_secret=$env:ES_TOTP_SECRET",
  "-input_file_path=$artifact",
  "-override=true",
  "-malware_block=false"
)

# Never stream or print provider output: current upstream action versions echo
# the assembled command, which includes all eSigner credentials.
$providerOutput = @()
$providerExitCode = -1
try {
  $providerOutput = @(& $java @arguments 2>&1)
  $providerExitCode = $LASTEXITCODE
} finally {
  # CodeSignTool may persist verbose command logs beside its jar. The hosted
  # runner is ephemeral, but remove them immediately so credentials cannot be
  # picked up by a later artifact or diagnostic step.
  $providerLogs = Join-Path $toolRoot "logs"
  if (Test-Path -LiteralPath $providerLogs -PathType Container) {
    Remove-Item -LiteralPath $providerLogs -Recurse -Force
  }
}
$providerText = ($providerOutput | ForEach-Object { $_.ToString() }) -join "`n"
$providerFailure = $providerText -match "(?im)Error|Exception|Missing required option|Unmatched argument"
if ($providerExitCode -ne 0 -or $providerFailure) {
  throw "SSL.com eSigner failed for $([System.IO.Path]::GetFileName($artifact)) (exit $providerExitCode). Provider output was suppressed to protect release secrets."
}

$signature = Get-AuthenticodeSignature -LiteralPath $artifact
if ($signature.SignatureType -ne "Authenticode" -or $signature.Status -ne "Valid") {
  throw "eSigner returned without a valid Authenticode signature for $([System.IO.Path]::GetFileName($artifact)): $($signature.Status)"
}

if ($env:OATS_SIGNING_AUDIT_PATH) {
  $auditPath = [System.IO.Path]::GetFullPath($env:OATS_SIGNING_AUDIT_PATH)
  $auditDirectory = Split-Path -Parent $auditPath
  if ($auditDirectory) {
    New-Item -ItemType Directory -Force -Path $auditDirectory | Out-Null
  }
  $record = [ordered]@{
    path = $artifact
    file = [System.IO.Path]::GetFileName($artifact)
    sha256 = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
    signer = $signature.SignerCertificate.Subject
  }
  Add-Content -LiteralPath $auditPath -Value ($record | ConvertTo-Json -Compress) -Encoding utf8
}

Write-Output "Authenticode signed $([System.IO.Path]::GetFileName($artifact))."
