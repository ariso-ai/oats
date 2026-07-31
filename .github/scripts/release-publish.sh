#!/usr/bin/env bash
#
# Build the Tauri updater manifest (latest.json) and publish the release
# artifacts (macOS updater tarball, Windows x64 NSIS installer, DMG, manifest)
# to Cloudflare R2. The Windows MSI is validated as part of the handoff but is
# uploaded only as a direct-download GitHub Release asset by the workflow.
#
# Invoked by the publish job in .github/workflows/release.yaml after the
# release job has uploaded the bundler outputs under bundle/.
#
# Required environment:
#   RELEASE_TAG    release tag (e.g. v0.3.1); leading 'v' is stripped for VERSION
#   R2_ENDPOINT    https://<account-id>.r2.cloudflarestorage.com
#   R2_BUCKET      bucket backing the public pub-...r2.dev domain
#   AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY    R2 API token credentials
# Optional environment:
#   RELEASE_BODY   release notes (may be multi-line); defaults to empty
#   RELEASE_NAME   release title; "[mandatory]" in it marks a forced update
#   RELEASE_PUBLISH_DRY_RUN=1   generate and validate latest.json without upload
set -euo pipefail

# Fail fast with an actionable message if a required secret is missing. An
# empty R2_ENDPOINT otherwise surfaces deep inside the AWS CLI as the opaque
# 'Bad value for --endpoint-url "": scheme is missing.', which gives no hint
# that the real problem is an unset secret in the 'release' environment.
require_env() {
  local missing=0 name
  for name in "$@"; do
    if [[ -z "${!name:-}" ]]; then
      echo "Missing required environment variable: ${name}" >&2
      missing=1
    fi
  done
  if [[ "$missing" -ne 0 ]]; then
    echo "Set these as secrets in the 'release' GitHub environment" \
      "(see CONTRIBUTING -> One-time setup in the repo)." >&2
    exit 1
  fi
}

require_env RELEASE_TAG
if [[ "${RELEASE_PUBLISH_DRY_RUN:-0}" != "1" ]]; then
  require_env R2_ENDPOINT R2_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY
fi

# R2_BUCKET must be the bucket NAME (e.g. ariso-app), not the public
# pub-<hash>.r2.dev domain that serves it. Bucket names can't contain dots,
# and the AWS CLI surfaces the mixup as an unhelpful InvalidBucketName error
# from CreateMultipartUpload.
if [[ "${RELEASE_PUBLISH_DRY_RUN:-0}" != "1" && "${R2_BUCKET:-}" == *.* ]]; then
  echo "R2_BUCKET looks like a domain ('${R2_BUCKET}'), not a bucket name." >&2
  echo "Set it to the R2 bucket name (no dots), e.g. via" \
    "'gh variable set R2_BUCKET --env release --body <bucket>'." >&2
  exit 1
fi

# Ensure the AWS CLI is available on the self-hosted runner.
if [[ "${RELEASE_PUBLISH_DRY_RUN:-0}" != "1" ]] && ! command -v aws >/dev/null 2>&1; then
  echo "AWS CLI not found on runner; install with 'brew install awscli'." >&2
  exit 1
fi

BUNDLE_DIR="src-tauri/target/release/bundle"

# Locate the updater artifacts the bundler produced. Tauri v2 writes these to
# bundle/macos/. Cached builds can leave stale tarballs alongside the fresh
# one, so require exactly one match and fail loudly otherwise.
mapfile -t TARBALLS < <(find "${BUNDLE_DIR}/macos" -maxdepth 1 -type f -name '*.app.tar.gz' | sort)
if [[ "${#TARBALLS[@]}" -ne 1 ]]; then
  echo "Expected exactly 1 updater tarball, found ${#TARBALLS[@]}:" >&2
  printf ' - %s\n' "${TARBALLS[@]}" >&2
  exit 1
fi
TARBALL="${TARBALLS[0]}"
SIGFILE="${TARBALL}.sig"

mapfile -t DMGS < <(find "${BUNDLE_DIR}/dmg" -maxdepth 1 -type f -name '*.dmg' | sort)
if [[ "${#DMGS[@]}" -ne 1 ]]; then
  echo "Expected exactly 1 DMG, found ${#DMGS[@]}." >&2
  exit 1
fi
DMG="${DMGS[0]}"

mapfile -t WINDOWS_INSTALLERS < <(find "${BUNDLE_DIR}/windows" -maxdepth 1 -type f -name '*.exe' | sort)
if [[ "${#WINDOWS_INSTALLERS[@]}" -ne 1 ]]; then
  echo "Expected exactly 1 Windows x64 NSIS installer, found ${#WINDOWS_INSTALLERS[@]}:" >&2
  printf ' - %s\n' "${WINDOWS_INSTALLERS[@]}" >&2
  exit 1
fi
WINDOWS_INSTALLER="${WINDOWS_INSTALLERS[0]}"
WINDOWS_SIGFILE="${WINDOWS_INSTALLER}.sig"
if [[ ! -f "$WINDOWS_SIGFILE" ]]; then
  echo "Missing Windows updater signature: ${WINDOWS_SIGFILE}" >&2
  exit 1
fi

# MSI is a direct-download GitHub Release asset only. Requiring it in the
# publish handoff prevents a partial public release, but it is deliberately
# excluded from latest.json and does not receive a Tauri updater signature.
mapfile -t WINDOWS_MSIS < <(find "${BUNDLE_DIR}/windows" -maxdepth 1 -type f -name '*.msi' | sort)
if [[ "${#WINDOWS_MSIS[@]}" -ne 1 ]]; then
  echo "Expected exactly 1 Windows x64 MSI installer, found ${#WINDOWS_MSIS[@]}:" >&2
  printf ' - %s\n' "${WINDOWS_MSIS[@]}" >&2
  exit 1
fi

# The version in tauri.conf.json (strip leading 'v' from tag).
VERSION="${RELEASE_TAG#v}"
if [[ ! "$VERSION" =~ ^[0-9A-Za-z][0-9A-Za-z._-]*$ ]]; then
  echo "Release tag produces an unsafe R2 path component: ${VERSION}" >&2
  exit 1
fi

# Updater payloads are immutable and versioned. A client that cached an older
# latest.json therefore keeps downloading the exact bytes covered by that
# manifest's signature even if a later publish is interrupted. Stable aliases
# remain human download links and are updated only after latest.json is live.
PUBLIC_R2_BASE="https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev"
MAC_SHA256=$(shasum -a 256 "$TARBALL" | awk '{print $1}')
MAC_ASSET_KEY="desktop/releases/${VERSION}/oats-${MAC_SHA256}.app.tar.gz"
MAC_ASSET_URL="${PUBLIC_R2_BASE}/${MAC_ASSET_KEY}"
WINDOWS_SHA256=$(shasum -a 256 "$WINDOWS_INSTALLER" | awk '{print $1}')
WINDOWS_ASSET_KEY="desktop/releases/${VERSION}/oats-windows-x86_64-${WINDOWS_SHA256}.exe"
WINDOWS_ASSET_URL="${PUBLIC_R2_BASE}/${WINDOWS_ASSET_KEY}"

# Read the detached signature contents (single line of base64).
MAC_SIGNATURE=$(cat "$SIGFILE")
WINDOWS_SIGNATURE=$(cat "$WINDOWS_SIGFILE")

# Mandatory flag: derived from the release title containing "[mandatory]".
if [[ "${RELEASE_NAME:-}" == *"[mandatory]"* ]]; then
  MANDATORY="true"
else
  MANDATORY="false"
fi

# release-please appends a markdown link to the specific commit on every
# changelog entry, e.g. "... ([9553cd9](https://github.com/.../commit/<sha>))".
# These are noise in the in-app updater dialog, so strip the trailing commit
# links while leaving the version-compare header link intact.
NOTES=$(printf '%s' "${RELEASE_BODY:-}" \
  | sed -E 's/ \(\[[0-9a-f]+\]\([^)]*\/commit\/[^)]*\)\)//g')

# Build the manifest using jq to ensure valid JSON escaping of the
# (possibly multi-line) release body.
jq -n \
  --arg version "$VERSION" \
  --arg notes "$NOTES" \
  --arg pub_date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson mandatory "$MANDATORY" \
  --arg mac_signature "$MAC_SIGNATURE" \
  --arg mac_url "$MAC_ASSET_URL" \
  --arg windows_signature "$WINDOWS_SIGNATURE" \
  --arg windows_url "$WINDOWS_ASSET_URL" \
  '{
    version: $version,
    notes: $notes,
    pub_date: $pub_date,
    mandatory: $mandatory,
    platforms: {
      "darwin-aarch64": {
        signature: $mac_signature,
        url: $mac_url
      },
      "windows-x86_64": {
        signature: $windows_signature,
        url: $windows_url
      }
    }
  }' > latest.json

if [[ "${RELEASE_PUBLISH_DRY_RUN:-0}" == "1" ]]; then
  jq -e '
    (.platforms["darwin-aarch64"].signature | length > 0) and
    (.platforms["darwin-aarch64"].url | length > 0) and
    (.platforms["windows-x86_64"].signature | length > 0) and
    (.platforms["windows-x86_64"].url | endswith(".exe"))
  ' latest.json >/dev/null
  echo "Dry run: validated macOS + Windows NSIS updater manifest; no R2 objects were published."
  exit 0
fi

NOCACHE="no-cache, max-age=0, must-revalidate"
IMMUTABLE_CACHE="public, max-age=31536000, immutable"

# Upload immutable updater payloads BEFORE the manifest that references them,
# so a client reading the new latest.json never points at a missing object.
aws s3 cp "$TARBALL" "s3://${R2_BUCKET}/${MAC_ASSET_KEY}" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/gzip \
  --cache-control "$IMMUTABLE_CACHE"

aws s3 cp "$WINDOWS_INSTALLER" "s3://${R2_BUCKET}/${WINDOWS_ASSET_KEY}" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/vnd.microsoft.portable-executable \
  --cache-control "$IMMUTABLE_CACHE"

# Convenience aliases are not updater targets. Publish both before latest.json
# so the manifest remains the final atomic release-visibility step.
aws s3 cp "$DMG" "s3://${R2_BUCKET}/desktop/oats.dmg" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/x-apple-diskimage \
  --cache-control "$NOCACHE"

aws s3 cp "$WINDOWS_INSTALLER" "s3://${R2_BUCKET}/desktop/oats-windows-x86_64.exe" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/vnd.microsoft.portable-executable \
  --cache-control "$NOCACHE"

# Publish the combined platform manifest last. At this point both immutable
# updater URLs and both human download aliases already exist.
aws s3 cp latest.json "s3://${R2_BUCKET}/desktop/latest.json" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/json \
  --cache-control "$NOCACHE"
