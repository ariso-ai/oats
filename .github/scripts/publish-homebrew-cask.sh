#!/usr/bin/env bash
#
# Publish the Homebrew install asset and commit the generated cask fields. This
# runs after the signed DMG has been built, so the cask version/checksum are
# derived from the release tag and the actual installer bytes.
set -euo pipefail

if [[ -z "${RELEASE_TAG:-}" ]]; then
  echo "Missing required environment variable: RELEASE_TAG" >&2
  exit 1
fi

VERSION="${RELEASE_TAG#v}"
BUNDLE_DMG_DIR="src-tauri/target/release/bundle/dmg"

DMG_COUNT="$(find "$BUNDLE_DMG_DIR" -maxdepth 1 -type f -name '*.dmg' | wc -l | tr -d ' ')"
if [[ "$DMG_COUNT" != "1" ]]; then
  echo "Expected exactly 1 DMG, found ${DMG_COUNT}:" >&2
  find "$BUNDLE_DMG_DIR" -maxdepth 1 -type f -name '*.dmg' -print >&2
  exit 1
fi

DMG="$(find "$BUNDLE_DMG_DIR" -maxdepth 1 -type f -name '*.dmg')"
DMG_SHA256="$(shasum -a 256 "$DMG" | awk '{print $1}')"
HOMEBREW_DMG_ASSET="oats.dmg"
trap 'rm -f "$HOMEBREW_DMG_ASSET"' EXIT

cp "$DMG" "$HOMEBREW_DMG_ASSET"
gh release upload "$RELEASE_TAG" "$HOMEBREW_DMG_ASSET" --clobber

git fetch origin main:main
git switch main

# Rebase onto the freshest main before committing, then replay the deterministic
# cask update so concurrent main changes do not produce an avoidable stale-base
# commit.
.github/scripts/update-homebrew-cask.sh "$VERSION" "$DMG_SHA256"
git checkout -- Casks/oats.rb
git fetch origin main:refs/remotes/origin/main
git rebase origin/main
.github/scripts/update-homebrew-cask.sh "$VERSION" "$DMG_SHA256"

if git diff --quiet Casks/oats.rb; then
  echo "Homebrew cask already matches ${VERSION}."
  exit 0
fi

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add Casks/oats.rb
git commit -m "chore: update Homebrew cask for ${RELEASE_TAG}"

for attempt in 1 2 3; do
  git fetch origin main:refs/remotes/origin/main
  git rebase origin/main

  if git push origin main; then
    exit 0
  fi

  sleep_seconds=$((2 ** attempt))
  echo "Push failed; retrying after ${sleep_seconds}s (attempt ${attempt}/3)." >&2
  sleep "$sleep_seconds"
done

echo "Failed to push Homebrew cask update after 3 attempts." >&2
exit 1
