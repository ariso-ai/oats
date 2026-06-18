#!/usr/bin/env bash
set -euo pipefail

# Compose the release DMG from Tauri's built app bundle without scripting
# Finder, then normalize the generated filename for the publish pipeline.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${VERSION:-$(cd "${ROOT_DIR}" && node -p 'JSON.parse(require("fs").readFileSync("package.json", "utf8")).version')}"
DMG_ARCH_SUFFIX="${DMG_ARCH_SUFFIX:-aarch64}"
APP_PATH="${APP_PATH:-${ROOT_DIR}/src-tauri/target/release/bundle/macos/oats.app}"
OUTPUT_PATH="${OUTPUT_PATH:-${ROOT_DIR}/src-tauri/target/release/bundle/dmg/oats_${VERSION}_${DMG_ARCH_SUFFIX}.dmg}"
VOLUME_NAME="${VOLUME_NAME:-oats}"
CREATE_DMG_BIN="${CREATE_DMG_BIN:-${ROOT_DIR}/node_modules/.bin/create-dmg}"

if [[ ! -d "${APP_PATH}" ]]; then
  echo "Missing app bundle: ${APP_PATH}" >&2
  echo "Run: npm run tauri:build -- --bundles app -- --features prod-api" >&2
  exit 1
fi

if [[ ! -x "${CREATE_DMG_BIN}" ]]; then
  echo "Missing create-dmg executable: ${CREATE_DMG_BIN}" >&2
  echo "Run: npm ci" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oats-dmg.XXXXXX")"

# Keep temporary create-dmg output isolated so its opinionated filename cannot
# leak into the release bundle directory or collide with stale artifacts.
cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

mkdir -p "$(dirname "${OUTPUT_PATH}")"
rm -f "${OUTPUT_PATH}"

"${CREATE_DMG_BIN}" "${APP_PATH}" "${WORK_DIR}" \
  --overwrite \
  --no-code-sign \
  --dmg-title="${VOLUME_NAME}"

DMGS=()
while IFS= read -r dmg_path; do
  DMGS+=("${dmg_path}")
done < <(find "${WORK_DIR}" -maxdepth 1 -type f -name '*.dmg' | sort)
if [[ "${#DMGS[@]}" -ne 1 ]]; then
  echo "Expected create-dmg to produce exactly 1 DMG, found ${#DMGS[@]}." >&2
  printf '%s\n' "${DMGS[@]}" >&2
  exit 1
fi

mv "${DMGS[0]}" "${OUTPUT_PATH}"
echo "Composed DMG: ${OUTPUT_PATH}"
