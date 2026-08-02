#!/usr/bin/env bash
#
# Shared helpers for the per-platform R2 publishers, release-publish-macos.sh
# and release-publish-windows.sh.
#
# macOS and Windows publish independently: they build on different runners,
# hold different signing credentials, and live in different workflows, so a
# Windows Authenticode failure must not hold back a finished macOS release.
# That independence is why each platform owns its own updater manifest — a
# Tauri static manifest carries a single top-level `version`, so one combined
# file cannot honestly describe macOS on 0.19.0 while Windows is still 0.18.0.
#
# Source this file; it defines constants and functions but performs no work.

# These constants are consumed by the sourcing scripts, not by this file.
# shellcheck disable=SC2034

# Public r2.dev managed domain in front of R2_BUCKET. Keep in sync with
# `r2_base!` in src-tauri/src/model_manager.rs and plugins.updater.endpoints in
# src-tauri/tauri.conf.json.
PUBLIC_R2_BASE="https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev"

NOCACHE="no-cache, max-age=0, must-revalidate"
IMMUTABLE_CACHE="public, max-age=31536000, immutable"

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

# RELEASE_PUBLISH_DRY_RUN=1 makes a publisher build and validate its updater
# manifest and then stop, so the manifest logic can be exercised locally without
# R2 credentials and without writing a single object.
release_publish_dry_run() {
  [[ "${RELEASE_PUBLISH_DRY_RUN:-0}" == "1" ]]
}

# Validate the R2 configuration and the AWS CLI before anything is uploaded, so
# a misconfigured environment fails before a partial publish.
r2_preflight() {
  if release_publish_dry_run; then
    return 0
  fi

  require_env R2_ENDPOINT R2_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

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

  if ! command -v aws >/dev/null 2>&1; then
    echo "AWS CLI not found on runner; install it before publishing." >&2
    exit 1
  fi
}

# Echo the version implied by RELEASE_TAG (leading 'v' stripped), rejecting any
# tag that would escape the desktop/releases/<version>/ prefix.
release_version() {
  require_env RELEASE_TAG
  local version="${RELEASE_TAG#v}"
  if [[ ! "$version" =~ ^[0-9A-Za-z][0-9A-Za-z._-]*$ ]]; then
    echo "Release tag produces an unsafe R2 path component: ${version}" >&2
    exit 1
  fi
  printf '%s' "$version"
}

# find_one <dir> <name-glob> <description>
#
# Echo the single file matching the glob. Cached builds can leave a stale
# artifact alongside the fresh one, so an ambiguous match must never be
# resolved by silently picking the first.
find_one() {
  local dir="$1" glob="$2" description="$3"
  if [[ ! -d "$dir" ]]; then
    echo "Expected ${description} in ${dir}, but that directory does not exist." >&2
    exit 1
  fi

  local matches=()
  mapfile -t matches < <(find "$dir" -maxdepth 1 -type f -name "$glob" | sort)
  if [[ "${#matches[@]}" -ne 1 ]]; then
    echo "Expected exactly 1 ${description} in ${dir}, found ${#matches[@]}:" >&2
    printf ' - %s\n' "${matches[@]}" >&2
    exit 1
  fi
  printf '%s' "${matches[0]}"
}

# Echo the detached updater signature path beside <artifact>, or fail loudly.
# The signature is generated in an earlier job, so its absence means an artifact
# handoff dropped it rather than that the file was never produced.
sig_for() {
  local sig="$1.sig"
  if [[ ! -f "$sig" ]]; then
    echo "Missing updater signature: ${sig}" >&2
    exit 1
  fi
  printf '%s' "$sig"
}

# `shasum` ships with Perl, so it is present on both the macOS and the Ubuntu
# runner; `sha256sum` is the coreutils fallback for anything else.
sha256_of() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

# write_manifest <out-file> <version> <platform-key> <signature> <url>
#
# Emit a Tauri static updater manifest describing exactly one platform. Reads
# RELEASE_NAME (a "[mandatory]" marker forces the update) and RELEASE_BODY (the
# release notes) from the environment; both are optional.
write_manifest() {
  local out="$1" version="$2" platform="$3" signature="$4" url="$5"

  local mandatory="false"
  if [[ "${RELEASE_NAME:-}" == *"[mandatory]"* ]]; then
    mandatory="true"
  fi

  # release-please appends a markdown link to the specific commit on every
  # changelog entry, e.g. "... ([9553cd9](https://github.com/.../commit/<sha>))".
  # These are noise in the in-app updater dialog, so strip the trailing commit
  # links while leaving the version-compare header link intact.
  local notes
  notes=$(printf '%s' "${RELEASE_BODY:-}" \
    | sed -E 's/ \(\[[0-9a-f]+\]\([^)]*\/commit\/[^)]*\)\)//g')

  # jq guarantees valid JSON escaping of the (possibly multi-line) notes.
  jq -n \
    --arg version "$version" \
    --arg notes "$notes" \
    --arg pub_date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --argjson mandatory "$mandatory" \
    --arg platform "$platform" \
    --arg signature "$signature" \
    --arg url "$url" \
    '{
      version: $version,
      notes: $notes,
      pub_date: $pub_date,
      mandatory: $mandatory,
      platforms: {
        ($platform): {
          signature: $signature,
          url: $url
        }
      }
    }' > "$out"
}

# r2_cp <local-file> <key> <content-type> <cache-control>
r2_cp() {
  aws s3 cp "$1" "s3://${R2_BUCKET}/$2" \
    --endpoint-url "$R2_ENDPOINT" \
    --content-type "$3" \
    --cache-control "$4"
}
