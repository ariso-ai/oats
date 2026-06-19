#!/usr/bin/env bash
#
# Build the Tauri updater manifest (latest.json) and publish the release
# artifacts (updater tarball, DMG, manifest) to Cloudflare R2.
#
# Invoked by the publish job in .github/workflows/release.yaml after the
# release job has uploaded the bundler outputs (restored under
# src-tauri/target/release/bundle/).
#
# Required environment:
#   RELEASE_TAG    release tag (e.g. v0.3.1); leading 'v' is stripped for VERSION
#   R2_ENDPOINT    https://<account-id>.r2.cloudflarestorage.com
#   R2_BUCKET      bucket backing the public pub-...r2.dev domain
#   AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY    R2 API token credentials
# Optional environment:
#   RELEASE_BODY   release notes (may be multi-line); defaults to empty
#   RELEASE_NAME   release title; "[mandatory]" in it marks a forced update
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
      "(see README -> Signing & Notarization)." >&2
    exit 1
  fi
}

require_env RELEASE_TAG R2_ENDPOINT R2_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

# R2_BUCKET must be the bucket NAME (e.g. ariso-app), not the public
# pub-<hash>.r2.dev domain that serves it. Bucket names can't contain dots,
# and the AWS CLI surfaces the mixup as an unhelpful InvalidBucketName error
# from CreateMultipartUpload.
if [[ "$R2_BUCKET" == *.* ]]; then
  echo "R2_BUCKET looks like a domain ('${R2_BUCKET}'), not a bucket name." >&2
  echo "Set it to the R2 bucket name (no dots), e.g. via" \
    "'gh variable set R2_BUCKET --env release --body <bucket>'." >&2
  exit 1
fi

# Ensure the AWS CLI is available on the self-hosted runner.
if ! command -v aws >/dev/null 2>&1; then
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

# The version in tauri.conf.json (strip leading 'v' from tag).
VERSION="${RELEASE_TAG#v}"

# Asset URL is the stable R2 path the updater downloads the payload from. The
# object at this key is overwritten by the upload below.
ASSET_URL="https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/oats.app.tar.gz"

# Read the detached signature contents (single line of base64).
SIG=$(cat "$SIGFILE")

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
  --arg signature "$SIG" \
  --arg url "$ASSET_URL" \
  '{
    version: $version,
    notes: $notes,
    pub_date: $pub_date,
    mandatory: $mandatory,
    platforms: {
      "darwin-aarch64": {
        signature: $signature,
        url: $url
      }
    }
  }' > latest.json

NOCACHE="no-cache, max-age=0, must-revalidate"

# Upload payloads BEFORE the manifest that references them, so a client reading
# the new latest.json never points at a missing object.
aws s3 cp "$TARBALL" "s3://${R2_BUCKET}/desktop/oats.app.tar.gz" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/gzip \
  --cache-control "$NOCACHE"

aws s3 cp "$DMG" "s3://${R2_BUCKET}/desktop/oats.dmg" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/x-apple-diskimage \
  --cache-control "$NOCACHE"

aws s3 cp latest.json "s3://${R2_BUCKET}/desktop/latest.json" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/json \
  --cache-control "$NOCACHE"
