#!/usr/bin/env bash
set -euo pipefail

# Compose the release DMG from Tauri's built app bundle while preserving control
# over Finder-only layout details that Tauri's public DMG config does not expose.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${VERSION:-$(cd "${ROOT_DIR}" && node -p 'JSON.parse(require("fs").readFileSync("package.json", "utf8")).version')}"
DMG_ARCH_SUFFIX="${DMG_ARCH_SUFFIX:-aarch64}"
APP_PATH="${APP_PATH:-${ROOT_DIR}/src-tauri/target/release/bundle/macos/oats.app}"
APP_BUNDLE_NAME="$(basename "${APP_PATH}")"
BACKGROUND_PATH="${BACKGROUND_PATH:-${ROOT_DIR}/src-tauri/assets/dmg-background.png}"
VOLUME_ICON_PATH="${VOLUME_ICON_PATH:-${ROOT_DIR}/src-tauri/icons/icon.icns}"
OUTPUT_PATH="${OUTPUT_PATH:-${ROOT_DIR}/src-tauri/target/release/bundle/dmg/oats_${VERSION}_${DMG_ARCH_SUFFIX}.dmg}"
VOLUME_NAME="${VOLUME_NAME:-oats}"
ICON_SIZE="${DMG_ICON_SIZE:-80}"
APP_ICON_X="${DMG_APP_ICON_X:-210}"
APP_ICON_Y="${DMG_APP_ICON_Y:-270}"
APPLICATIONS_ICON_X="${DMG_APPLICATIONS_ICON_X:-645}"
APPLICATIONS_ICON_Y="${DMG_APPLICATIONS_ICON_Y:-280}"
WINDOW_WIDTH="${DMG_WINDOW_WIDTH:-820}"
WINDOW_HEIGHT="${DMG_WINDOW_HEIGHT:-500}"

if [[ ! -d "${APP_PATH}" ]]; then
  echo "Missing app bundle: ${APP_PATH}" >&2
  echo "Run: npm run tauri:build -- --bundles app -- --features prod-api" >&2
  exit 1
fi

if [[ ! -f "${BACKGROUND_PATH}" ]]; then
  echo "Missing DMG background: ${BACKGROUND_PATH}" >&2
  exit 1
fi

if [[ ! -f "${VOLUME_ICON_PATH}" ]]; then
  echo "Missing DMG volume icon: ${VOLUME_ICON_PATH}" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oats-dmg.XXXXXX")"
STAGING_DIR="${WORK_DIR}/staging"
RW_DMG="${WORK_DIR}/rw.dmg"
MOUNT_DIR="/Volumes/${VOLUME_NAME}"
BACKGROUND_NAME="$(basename "${BACKGROUND_PATH}")"

# Detach and remove temporary DMG staging resources even when Finder or hdiutil
# exits early, so repeated release attempts do not inherit stale mount state.
cleanup() {
  hdiutil detach "${MOUNT_DIR}" >/dev/null 2>&1 || true
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

mkdir -p "${STAGING_DIR}/.background" "$(dirname "${OUTPUT_PATH}")"
cp -R "${APP_PATH}" "${STAGING_DIR}/"
cp "${BACKGROUND_PATH}" "${STAGING_DIR}/.background/${BACKGROUND_NAME}"
rm -f "${OUTPUT_PATH}"

hdiutil create \
  -srcfolder "${STAGING_DIR}" \
  -volname "${VOLUME_NAME}" \
  -fs HFS+ \
  -format UDRW \
  -ov \
  "${RW_DMG}" >/dev/null

hdiutil detach "${MOUNT_DIR}" >/dev/null 2>&1 || true
hdiutil attach "${RW_DMG}" -readwrite -noverify -noautoopen -quiet

# Finder persists the icon size, positions, and background into .DS_Store; this
# keeps the oatmeal bowl composition stable when users open the packaged DMG.
osascript <<APPLESCRIPT
tell application "Finder"
  tell disk "${VOLUME_NAME}"
    open
    make new alias file to POSIX file "/Applications" at container window with properties {name:"Applications"}
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set bounds of container window to {10, 60, 10 + ${WINDOW_WIDTH}, 60 + ${WINDOW_HEIGHT}}

    set opts to icon view options of container window
    set icon size of opts to ${ICON_SIZE}
    set text size of opts to 16
    set arrangement of opts to not arranged
    set background picture of opts to file ".background:${BACKGROUND_NAME}"

    set position of item "${APP_BUNDLE_NAME}" to {${APP_ICON_X}, ${APP_ICON_Y}}
    set position of item "Applications" to {${APPLICATIONS_ICON_X}, ${APPLICATIONS_ICON_Y}}
    set extension hidden of item "${APP_BUNDLE_NAME}" to true
    update without registering applications
    delay 2
    close
    open
    delay 1
    close
  end tell
end tell
APPLESCRIPT

for _ in {1..10}; do
  [[ -f "${MOUNT_DIR}/.DS_Store" ]] && break
  sleep 1
done

if [[ ! -f "${MOUNT_DIR}/.DS_Store" ]]; then
  echo "Finder did not write ${MOUNT_DIR}/.DS_Store" >&2
  exit 1
fi

# Give the mounted disk image the app icon after Finder has saved the window
# layout, preventing the layout pass from clearing the volume icon metadata.
cp "${VOLUME_ICON_PATH}" "${MOUNT_DIR}/.VolumeIcon.icns"
SetFile -c icnC "${MOUNT_DIR}/.VolumeIcon.icns"
SetFile -a C "${MOUNT_DIR}"

sync
hdiutil detach "${MOUNT_DIR}" -quiet
hdiutil convert "${RW_DMG}" -format UDZO -imagekey zlib-level=9 -o "${OUTPUT_PATH}" >/dev/null
echo "Composed DMG: ${OUTPUT_PATH}"
