#!/usr/bin/env bash
#
# Publish the macOS release to Cloudflare R2: the immutable updater tarball, the
# human-download DMG alias, and the darwin-aarch64 updater manifest.
#
# Invoked by the `publish` job in .github/workflows/release.yaml after the
# `release` job has uploaded the bundler outputs under bundle/. Windows ships
# from .github/workflows/release-windows.yaml via release-publish-windows.sh and
# never touches any object this script writes.
#
# Required environment:
#   RELEASE_TAG    release tag (e.g. v0.3.1); leading 'v' is stripped for VERSION
#   R2_ENDPOINT    https://<account-id>.r2.cloudflarestorage.com
#   R2_BUCKET      bucket backing the public pub-...r2.dev domain
#   AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY    R2 API token credentials
# Optional environment:
#   RELEASE_BODY   release notes (may be multi-line); defaults to empty
#   RELEASE_NAME   release title; "[mandatory]" in it marks a forced update
#   RELEASE_PUBLISH_DRY_RUN=1   generate and validate the manifest without upload
set -euo pipefail

# shellcheck source=.github/scripts/release-publish-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/release-publish-common.sh"

r2_preflight
VERSION="$(release_version)"

BUNDLE_DIR="src-tauri/target/release/bundle"

# Tauri v2 writes the updater artifacts to bundle/macos/.
TARBALL="$(find_one "${BUNDLE_DIR}/macos" '*.app.tar.gz' 'updater tarball')"
SIGFILE="$(sig_for "$TARBALL")"
DMG="$(find_one "${BUNDLE_DIR}/dmg" '*.dmg' 'DMG')"

# Updater payloads are immutable and versioned. A client that cached an older
# manifest therefore keeps downloading the exact bytes covered by that
# manifest's signature even if a later publish is interrupted. Stable aliases
# remain human download links and are updated only before the manifest goes
# live.
MAC_SHA256="$(sha256_of "$TARBALL")"
MAC_ASSET_KEY="desktop/releases/${VERSION}/oats-${MAC_SHA256}.app.tar.gz"
MAC_ASSET_URL="${PUBLIC_R2_BASE}/${MAC_ASSET_KEY}"

# The Homebrew cask (published by publish-homebrew-cask.sh) downloads the DMG
# from this same hash-addressed key, so a later release can never replace the
# bytes a merged cask PR points at the way a mutable alias could.
DMG_SHA256="$(sha256_of "$DMG")"
DMG_ASSET_KEY="desktop/releases/${VERSION}/oats-${DMG_SHA256}.dmg"

write_manifest latest-darwin-aarch64.json \
  "$VERSION" darwin-aarch64 "$(cat "$SIGFILE")" "$MAC_ASSET_URL"

if release_publish_dry_run; then
  jq -e '
    (.platforms["darwin-aarch64"].signature | length > 0) and
    (.platforms["darwin-aarch64"].url | endswith(".app.tar.gz"))
  ' latest-darwin-aarch64.json >/dev/null
  echo "Dry run: validated the macOS updater manifest; no R2 objects were published."
  exit 0
fi

# Upload the immutable updater payload BEFORE the manifest that references it,
# so a client reading the new manifest never points at a missing object.
r2_cp "$TARBALL" "$MAC_ASSET_KEY" application/gzip "$IMMUTABLE_CACHE"

# Publish the DMG under its immutable, hash-addressed key first — the cask PR
# opened later in this job reads DMG_SHA256 off this exact object. The mutable
# desktop/oats.dmg alias is not an updater or cask target; it only backs the
# direct "Download for macOS" link, so publish it before the manifest so the
# manifest remains the final atomic release-visibility step.
r2_cp "$DMG" "$DMG_ASSET_KEY" application/x-apple-diskimage "$IMMUTABLE_CACHE"
r2_cp "$DMG" desktop/oats.dmg application/x-apple-diskimage "$NOCACHE"

r2_cp latest-darwin-aarch64.json desktop/latest-darwin-aarch64.json \
  application/json "$NOCACHE"

# Apps ship their updater endpoints compiled in, so every client installed
# before the per-platform manifests existed still polls desktop/latest.json.
# Keep serving them a macOS-only copy of the same manifest. Windows never
# writes this object, so it can never describe a version macOS has not shipped.
r2_cp latest-darwin-aarch64.json desktop/latest.json \
  application/json "$NOCACHE"
