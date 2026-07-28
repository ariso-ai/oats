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

function Write-SafeSigningDiagnostic {
  param([Parameter(Mandatory = $true)][string]$Message)

  if ($env:OATS_SIGNING_DIAGNOSTIC_PATH) {
    $diagnosticPath = [System.IO.Path]::GetFullPath($env:OATS_SIGNING_DIAGNOSTIC_PATH)
    $diagnosticDirectory = Split-Path -Parent $diagnosticPath
    if ($diagnosticDirectory) {
      New-Item -ItemType Directory -Force -Path $diagnosticDirectory | Out-Null
    }
    Add-Content -LiteralPath $diagnosticPath -Value $Message -Encoding utf8
  }
}

$temporarySigningCopy = $null
try {
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
$artifactInfo = Get-Item -LiteralPath $artifact
Write-SafeSigningDiagnostic "Starting signer for $($artifactInfo.Name): path=$artifact, cwd=$((Get-Location).Path), readOnly=$($artifactInfo.IsReadOnly)"
$extension = [System.IO.Path]::GetExtension($artifact).ToLowerInvariant()
$artifactKind = "artifact"
$providerArtifact = $artifact
if ($extension -eq ".tmp") {
  $header = New-Object byte[] 2
  $stream = [System.IO.File]::OpenRead($artifact)
  try {
    $headerLength = $stream.Read($header, 0, 2)
  } finally {
    $stream.Dispose()
  }
  if ($headerLength -ne 2 -or $header[0] -ne 0x4d -or $header[1] -ne 0x5a) {
    throw "Refusing to sign a temporary file that is not a Windows PE executable."
  }
  # NSIS names its generated uninstaller nst*.tmp. CodeSignTool filters by
  # extension, so sign an exact .exe copy and put the signed PE bytes back at
  # the path Tauri supplied.
  $artifactKind = "nsis-uninstaller"
  $temporarySigningCopy = "$artifact.oats-uninstaller.exe"
  Copy-Item -LiteralPath $artifact -Destination $temporarySigningCopy -Force
  $providerArtifact = $temporarySigningCopy
} elseif ($extension -notin @(".exe", ".dll", ".msi")) {
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
  "-input_file_path=$providerArtifact",
  "-override=true",
  "-malware_block=false"
)

# Never stream or print provider output: current upstream action versions echo
# the assembled command, which includes all eSigner credentials.
$providerOutput = @()
$providerExitCode = -1
$providerInvocationError = $null
try {
  $providerOutput = @(& $java @arguments 2>&1)
  $providerExitCode = $LASTEXITCODE
} catch {
  $providerInvocationError = $_.Exception.Message
} finally {
  # CodeSignTool may persist verbose command logs beside its jar. The hosted
  # runner is ephemeral, but remove them immediately so credentials cannot be
  # picked up by a later artifact or diagnostic step.
  $providerLogs = Join-Path $toolRoot "logs"
  if (Test-Path -LiteralPath $providerLogs -PathType Container) {
    Remove-Item -LiteralPath $providerLogs -Recurse -Force -ErrorAction SilentlyContinue
  }
}
$providerText = ($providerOutput | ForEach-Object { $_.ToString() }) -join "`n"
$providerFailure = $providerText -match "(?im)Error|Exception|Missing required option|Unmatched argument"
if ($providerInvocationError -or $providerExitCode -ne 0 -or $providerFailure) {
  $safeProviderText = "$providerInvocationError`n$providerText"
  foreach ($secretName in @("ES_USERNAME", "ES_PASSWORD", "ES_CREDENTIAL_ID", "ES_TOTP_SECRET")) {
    $secretValue = [Environment]::GetEnvironmentVariable($secretName)
    if (-not [string]::IsNullOrEmpty($secretValue)) {
      $safeProviderText = $safeProviderText -replace [regex]::Escape($secretValue), "***"
    }
  }
  $safeProviderLines = @(
    $safeProviderText -split "\r?\n" |
      Where-Object {
        $_ -notmatch "(?i)(?:^|\s)-(?:username|password|credential_id|totp_secret)="
      } |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
      Select-Object -First 20
  )
  $safeProviderJoined = [string](($safeProviderLines -join " | ").Trim())
  $safeProviderSummary = if ($safeProviderJoined.Length -gt 2000) {
    $safeProviderJoined.Substring(0, 2000)
  } else {
    $safeProviderJoined
  }
  if ([string]::IsNullOrWhiteSpace($safeProviderSummary)) {
    $safeProviderSummary = "(provider returned no safe diagnostic text)"
  }
  Write-SafeSigningDiagnostic "Provider failure for $([System.IO.Path]::GetFileName($artifact)) (exit $providerExitCode): $safeProviderSummary"
  throw "SSL.com eSigner failed for $([System.IO.Path]::GetFileName($artifact)) (exit $providerExitCode). Provider output was suppressed to protect release secrets."
}

$signature = Get-AuthenticodeSignature -LiteralPath $providerArtifact
if ($signature.SignatureType -ne "Authenticode" -or $signature.Status -ne "Valid") {
  Write-SafeSigningDiagnostic "Authenticode validation failed for $([System.IO.Path]::GetFileName($artifact)): type=$($signature.SignatureType), status=$($signature.Status), message=$($signature.StatusMessage)"
  throw "eSigner returned without a valid Authenticode signature for $([System.IO.Path]::GetFileName($artifact)): $($signature.Status)"
}
if ($temporarySigningCopy) {
  Copy-Item -LiteralPath $temporarySigningCopy -Destination $artifact -Force
  Remove-Item -LiteralPath $temporarySigningCopy -Force -ErrorAction SilentlyContinue
  $signature = Get-AuthenticodeSignature -LiteralPath $artifact
  if ($signature.SignatureType -ne "Authenticode" -or $signature.Status -ne "Valid") {
    throw "The signed NSIS uninstaller did not survive copying back to its Tauri path."
  }
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
    kind = $artifactKind
    sha256 = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
    signer = $signature.SignerCertificate.Subject
  }
  Add-Content -LiteralPath $auditPath -Value ($record | ConvertTo-Json -Compress) -Encoding utf8
}

Write-Output "Authenticode signed $([System.IO.Path]::GetFileName($artifact))."
} catch {
  if ($temporarySigningCopy -and (Test-Path -LiteralPath $temporarySigningCopy)) {
    Remove-Item -LiteralPath $temporarySigningCopy -Force -ErrorAction SilentlyContinue
  }
  $safeMessage = $_.Exception.Message
  foreach ($secretName in @("ES_USERNAME", "ES_PASSWORD", "ES_CREDENTIAL_ID", "ES_TOTP_SECRET")) {
    $secretValue = [Environment]::GetEnvironmentVariable($secretName)
    if (-not [string]::IsNullOrEmpty($secretValue)) {
      $safeMessage = $safeMessage -replace [regex]::Escape($secretValue), "***"
    }
  }
  $safeMessage = $safeMessage -replace "(?i)-(?:username|password|credential_id|totp_secret)=\S+", "-credential=***"
  Write-SafeSigningDiagnostic "Unhandled signer failure for $([System.IO.Path]::GetFileName($FilePath)): $safeMessage"
  throw "Windows signing wrapper failed for $([System.IO.Path]::GetFileName($FilePath)). See the secret-scrubbed diagnostic record."
}
