#!/usr/bin/env bash
#
# Open a PR that updates the generated Homebrew cask fields. This runs after
# release-publish.sh has shipped the signed DMG to R2 (desktop/oats.dmg), so the
# cask checksum is computed from the very bytes that R2 now serves — the cask's
# `url` points straight at that R2 object, so there is no separate asset to
# upload here.
#
# `main` is a protected branch (changes must go through a pull request that
# passes the required Validate check), so we never push the cask update to main
# directly — that push is rejected with GH013 and the checksum never lands. We
# push a dedicated branch and open a PR for a maintainer to merge instead.
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

# Hash the local DMG: these are the same bytes release-publish.sh just uploaded
# to R2, so the checksum matches what `brew install` will download.
DMG="$(find "$BUNDLE_DMG_DIR" -maxdepth 1 -type f -name '*.dmg')"
DMG_SHA256="$(shasum -a 256 "$DMG" | awk '{print $1}')"

# Apply the deterministic cask update on a fresh branch off the latest main, so
# the PR diff is exactly the version/checksum bump for this release.
git fetch origin main
BRANCH="chore/homebrew-cask-${RELEASE_TAG}"
git switch -C "$BRANCH" origin/main

.github/scripts/update-homebrew-cask.sh "$VERSION" "$DMG_SHA256"

if git diff --quiet Casks/oats.rb; then
  echo "Homebrew cask already matches ${VERSION}; nothing to publish."
  exit 0
fi

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add Casks/oats.rb
git commit -m "chore: update Homebrew cask for ${RELEASE_TAG}"

# Force-push is safe: the branch is owned by this automated, deterministic flow,
# so a re-run of the same release should overwrite any earlier attempt.
git push --force-with-lease origin "$BRANCH"

PR_TITLE="chore: update Homebrew cask for ${RELEASE_TAG}"
PR_BODY="$(cat <<EOF
Automated cask checksum update for ${RELEASE_TAG}, opened by the release workflow
after the signed DMG was published.

- version: \`${VERSION}\`
- sha256: \`${DMG_SHA256}\`
EOF
)"

# A PR may already exist if this release is being re-run; the force-push above
# already refreshed its contents, so only create one when it is missing.
if gh pr view "$BRANCH" --json number >/dev/null 2>&1; then
  echo "PR for ${BRANCH} already exists; refreshed cask update."
else
  gh pr create --base main --head "$BRANCH" --title "$PR_TITLE" --body "$PR_BODY"
fi
