#!/usr/bin/env bash
set -euo pipefail

# Compose the release DMG from Tauri's built app bundle without scripting
# Finder, while preserving the Oats-specific installer background and layout.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${VERSION:-$(cd "${ROOT_DIR}" && node -p 'JSON.parse(require("fs").readFileSync("package.json", "utf8")).version')}"
DMG_ARCH_SUFFIX="${DMG_ARCH_SUFFIX:-aarch64}"
APP_PATH="${APP_PATH:-${ROOT_DIR}/src-tauri/target/release/bundle/macos/oats.app}"
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
APP_DMG_BIN="${APP_DMG_BIN:-${ROOT_DIR}/node_modules/.bin/appdmg}"

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

if [[ ! -x "${APP_DMG_BIN}" ]]; then
  echo "Missing appdmg executable: ${APP_DMG_BIN}" >&2
  echo "Run: npm ci" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oats-dmg.XXXXXX")"
CONFIG_PATH="${WORK_DIR}/appdmg.json"

# Keep temporary appdmg config isolated so repeated release attempts cannot
# inherit stale paths or layout metadata from an earlier compose run.
cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

OUTPUT_DIR="$(dirname "${OUTPUT_PATH}")"
mkdir -p "${OUTPUT_DIR}"
find "${OUTPUT_DIR}" -maxdepth 1 -type f -name '*.dmg' -exec rm -f {} +

CONFIG_PATH="${CONFIG_PATH}" \
VOLUME_NAME="${VOLUME_NAME}" \
VOLUME_ICON_PATH="${VOLUME_ICON_PATH}" \
BACKGROUND_PATH="${BACKGROUND_PATH}" \
ICON_SIZE="${ICON_SIZE}" \
WINDOW_WIDTH="${WINDOW_WIDTH}" \
WINDOW_HEIGHT="${WINDOW_HEIGHT}" \
APP_ICON_X="${APP_ICON_X}" \
APP_ICON_Y="${APP_ICON_Y}" \
APPLICATIONS_ICON_X="${APPLICATIONS_ICON_X}" \
APPLICATIONS_ICON_Y="${APPLICATIONS_ICON_Y}" \
APP_PATH="${APP_PATH}" \
node <<'NODE'
const fs = require("fs");

const numberFromEnv = (name) => {
  const value = Number(process.env[name]);
  if (!Number.isFinite(value)) {
    throw new Error(`${name} must be numeric`);
  }

  return value;
};

const config = {
  title: process.env.VOLUME_NAME,
  icon: process.env.VOLUME_ICON_PATH,
  background: process.env.BACKGROUND_PATH,
  "icon-size": numberFromEnv("ICON_SIZE"),
  format: "UDZO",
  filesystem: "HFS+",
  window: {
    size: {
      width: numberFromEnv("WINDOW_WIDTH"),
      height: numberFromEnv("WINDOW_HEIGHT"),
    },
  },
  contents: [
    {
      x: numberFromEnv("APP_ICON_X"),
      y: numberFromEnv("APP_ICON_Y"),
      type: "file",
      path: process.env.APP_PATH,
    },
    {
      x: numberFromEnv("APPLICATIONS_ICON_X"),
      y: numberFromEnv("APPLICATIONS_ICON_Y"),
      type: "link",
      path: "/Applications",
    },
  ],
};

fs.writeFileSync(process.env.CONFIG_PATH, `${JSON.stringify(config, null, 2)}\n`);
NODE

"${APP_DMG_BIN}" "${CONFIG_PATH}" "${OUTPUT_PATH}"
echo "Composed DMG: ${OUTPUT_PATH}"
