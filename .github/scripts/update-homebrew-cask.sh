#!/usr/bin/env bash
#
# Keep the in-repo Homebrew cask derived from release inputs. The version comes
# from RELEASE_TAG, and the checksum comes from the signed DMG bytes that the
# release workflow uploads as the versioned GitHub Release asset.
set -euo pipefail

if [[ "$#" -lt 2 || "$#" -gt 3 ]]; then
  echo "Usage: $0 <version> <dmg-sha256> [cask-path]" >&2
  exit 64
fi

VERSION="$1"
DMG_SHA256="$2"
CASK_PATH="${3:-Casks/oats.rb}"

ruby - "$VERSION" "$DMG_SHA256" "$CASK_PATH" <<'RUBY'
version, dmg_sha256, cask_path = ARGV
text = File.read(cask_path)
text = text.sub(/^  generated_version = .+$/, "  generated_version = \"#{version}\"")
text = text.sub(/^  generated_sha256 = .+$/, "  generated_sha256 = \"#{dmg_sha256}\"")
File.write(cask_path, text)
RUBY
