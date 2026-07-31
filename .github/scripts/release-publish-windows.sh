#!/usr/bin/env bash
#
# Publish the Windows x64 release to Cloudflare R2: the immutable Authenticode-
# signed NSIS installer, the human-download alias, and the windows-x86_64
# updater manifest.
#
# Invoked by the `publish-windows` job in
# .github/workflows/release-windows.yaml after `sign-windows-updater` has
# attached the Tauri updater signature. macOS ships from
# .github/workflows/release.yaml via release-publish-macos.sh and never touches
# any object this script writes — including desktop/latest.json, which stays
# macOS-only for clients built before the per-platform manifests existed.
#
# Required environment:
#   RELEASE_TAG    release tag (e.g. v0.3.1); leading 'v' is stripped for VERSION
#   R2_ENDPOINT    https://<account-id>.r2.cloudflarestorage.com
#   R2_BUCKET      bucket backing the public pub-...r2.dev domain
#   AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY    R2 API token credentials
#
# The MSI that travels in the same handoff is validated here but never uploaded:
# it ships only as a direct-download GitHub Release asset, is excluded from
# latest.json, and carries no Tauri updater signature.
#
# Optional environment:
#   RELEASE_BODY   release notes (may be multi-line); defaults to empty
#   RELEASE_NAME   release title; "[mandatory]" in it marks a forced update
#   RELEASE_PUBLISH_DRY_RUN=1   generate and validate the manifest without upload
set -euo pipefail

# shellcheck source=.github/scripts/release-publish-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/release-publish-common.sh"

r2_preflight
VERSION="$(release_version)"

BUNDLE_DIR="artifacts/windows"

INSTALLER="$(find_one "$BUNDLE_DIR" '*.exe' 'Windows x64 NSIS installer')"
SIGFILE="$(sig_for "$INSTALLER")"

# Requiring the MSI even though this script never uploads it keeps an incomplete
# handoff from producing a partial public release: the GitHub Release step that
# attaches it runs after this one.
find_one "$BUNDLE_DIR" '*.msi' 'Windows x64 MSI installer' >/dev/null

# Immutable, hash-addressed payload: these are the exact Authenticode-signed
# bytes the .sig covers, so a client that cached an older manifest keeps
# downloading bytes its signature still verifies.
WINDOWS_SHA256="$(sha256_of "$INSTALLER")"
WINDOWS_ASSET_KEY="desktop/releases/${VERSION}/oats-windows-x86_64-${WINDOWS_SHA256}.exe"
WINDOWS_ASSET_URL="${PUBLIC_R2_BASE}/${WINDOWS_ASSET_KEY}"

write_manifest latest-windows-x86_64.json \
  "$VERSION" windows-x86_64 "$(cat "$SIGFILE")" "$WINDOWS_ASSET_URL"

if release_publish_dry_run; then
  jq -e '
    (.platforms["windows-x86_64"].signature | length > 0) and
    (.platforms["windows-x86_64"].url | endswith(".exe"))
  ' latest-windows-x86_64.json >/dev/null
  echo "Dry run: validated the Windows updater manifest; no R2 objects were published."
  exit 0
fi

WINDOWS_CONTENT_TYPE=application/vnd.microsoft.portable-executable

# Upload the immutable updater payload BEFORE the manifest that references it,
# so a client reading the new manifest never points at a missing object.
r2_cp "$INSTALLER" "$WINDOWS_ASSET_KEY" "$WINDOWS_CONTENT_TYPE" "$IMMUTABLE_CACHE"

# The download alias is not an updater target, but publish it before the
# manifest so the manifest remains the final atomic release-visibility step.
r2_cp "$INSTALLER" desktop/oats-windows-x86_64.exe \
  "$WINDOWS_CONTENT_TYPE" "$NOCACHE"

r2_cp latest-windows-x86_64.json desktop/latest-windows-x86_64.json \
  application/json "$NOCACHE"
