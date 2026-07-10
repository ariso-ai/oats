---
date: 2026-07-10T08:00:19.3894570-07:00
type: handoff
status: ready
topic: Windows MSI installer implementation
plan: thoughts/shared/plans/PLAN-windows-msi-installer-spike.md
---

# Handoff: Windows MSI Installer Implementation

## Summary

Replaced the original MSI spike outline with an actual implementation plan. The plan now specifies
the exact files to add/change, the exact Tauri build command, the release workflow shape, artifact
assertions, and local/manual verification commands.

## Created Plan

- `thoughts/shared/plans/PLAN-windows-msi-installer-spike.md`

## Key Technical Decisions

- Keep NSIS as the primary consumer installer.
- Add MSI as an internal artifact in release CI.
- Build both Windows installers with the shared helper:

  ```powershell
  powershell -ExecutionPolicy Bypass -File scripts\build-windows-installers.ps1
  ```

- Add `src-tauri/tauri.windows.conf.json` with `bundle.resources: null` so Windows packaging does
  not need the macOS `mlx-swift_Cmlx.bundle` placeholder.
- Add `scripts/build-windows-sidecar.ps1` and `scripts/build-windows-installers.ps1`, using
  `x86_64-pc-windows-msvc` explicitly for sidecar and app bundle output.
- Do not publish MSI publicly until signing, updater, install, upgrade, and uninstall behavior are
  verified.

## Implementation Tasks

- Add `src-tauri/tauri.windows.conf.json`.
- Add `scripts/build-windows-sidecar.ps1`.
- Update `.github/workflows/desktop.yaml` to call the helper and remove placeholder creation.
- Update `.github/workflows/release.yaml` to build `nsis,msi`, verify artifacts, and upload
  `windows-msi-internal`.
- Update the Windows full-parity spec.
- Run local Windows verification commands.
- Update PR #173 description if these changes land in that PR.

## Notable Research Findings

- Tauri supports both NSIS and MSI; it does not require choosing MSI as the one recommended path.
- Tauri auto-merges `tauri.windows.conf.json`.
- Tauri `--bundles` accepts comma-separated bundles.
- MSI builds run only on Windows.
- MSI should remain internal until signing and updater behavior are proven.
- Targeted Windows builds write artifacts under
  `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/`, not `src-tauri/target/release/bundle/`.
- Local MSI smoke can hit WiX ICE failures if Windows Installer Service/VBScript validation support
  is unavailable; CI and manual install/upgrade/uninstall smoke tests remain the gating evidence.

## Local Verification Update

- `powershell -ExecutionPolicy Bypass -File scripts\build-windows-sidecar.ps1` passed on Windows.
- `powershell -ExecutionPolicy Bypass -File scripts\build-windows-installers.ps1` passed on Windows
  with a throwaway local Tauri signing key.
- Fresh artifacts were produced under `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/`:
  NSIS `.exe`, MSI `.msi`, and one `.sig` beside each installer.
- The local smoke emitted the expected updater key mismatch warning because the throwaway private
  key does not match the checked-in updater public key.

## Next Agent Notes

This is now intended to be implemented directly. Start by adding the Windows config and helper
script, then update CI. Do not spend another cycle deciding whether MSI is worth doing; the
implementation goal is an internal `windows-msi-internal` artifact alongside the existing NSIS
artifact.
