---
date: 2026-07-10T07:55:42.5745260-07:00
type: research
status: success
topic: Windows MSI installer for oats
sources:
  - repo
  - tauri-docs
  - microsoft-learn
---

# Research Handoff: Windows MSI Installer

## Research Question

Have we already researched creating an MSI for the oats Windows release, and what should the next MSI research/spike cover?

## Existing Repo Findings

- The current Windows full-parity design explicitly picked "Windows 11 first, NSIS installer first"; MSI/WiX is not part of the documented release target.
- The release workflow currently has a Windows packaging job named "Build Windows NSIS artifact" and runs `npm run tauri:build -- --bundles nsis -- --features prod-api`.
- The current `src-tauri/tauri.conf.json` has `"bundle": { "targets": "all" }`, but the release workflow overrides this to NSIS only for Windows.
- There is no `src-tauri/tauri.windows.conf.json` yet. A platform-specific config is likely the clean place to split macOS-only resources and add Windows-specific WiX/MSI settings.
- No dedicated MSI/WiX research handoff or MSI implementation plan was found in `docs/`, `thoughts/`, `.github/`, or source files.

## External Documentation Findings

### Tauri Windows Installer

- Tauri v2 supports Windows installers as MSI files through WiX Toolset v3 and setup EXEs through NSIS.
- Tauri notes that `.msi` packages can only be created on Windows because WiX only runs on Windows.
- Building MSI packages with Tauri requires the Windows VBSCRIPT optional feature to be enabled; otherwise `light.exe` can fail.
- Tauri's WiX customization path supports built-in config, WiX fragments, or replacing the installer template with a custom `.wxs` file.
- Tauri's Windows bundle config supports `msi` and `nsis` bundle target types.

### Tauri Updater

- With `createUpdaterArtifacts: true`, Tauri creates updater signatures for both Windows MSI and NSIS outputs.
- For Windows v2 updater artifacts, Tauri expects normal installers in `target/release/bundle/msi/` and `target/release/bundle/nsis/`, including `.sig` files.
- Windows updater `installMode` can be `passive`, `basicUi`, or `quiet`; Tauri recommends `passive` by default.

### Microsoft MSI Guidance

- Microsoft recommends deciding and testing per-user vs per-machine installation before release.
- MSI packages should have a servicing strategy before first deployment, including upgrade/repair/uninstall behavior.
- Microsoft recommends testing install, UI levels, repair, servicing, uninstall, command-line install, and privilege contexts.
- For public distribution, Microsoft recommends signing MSI/EXE installers and their PE files with a trusted code-signing certificate. Self-signed certs are for development/testing only.

## Recommendations

- Treat MSI as a separate packaging spike, not as already researched.
- Keep NSIS as the current PR's public-release path unless product requirements specifically need MSI.
- Add a Windows MSI CI lane only after validating locally on `windows-latest`:
  - `npm run tauri:build -- --bundles msi -- --features prod-api`
  - verify `src-tauri/target/release/bundle/msi/**`
  - verify updater `.sig` artifacts are generated with the existing Tauri signing key
- Add `src-tauri/tauri.windows.conf.json` before MSI work if Windows needs different bundle resources, WebView2 mode, signing, WiX config, or bundle targets than macOS.
- Decide install context deliberately:
  - per-user is friendlier for consumer installs and updater flows
  - per-machine may be better for enterprise deployment but raises UAC/admin and update-mode questions
- Smoke test both first install and upgrade from the existing NSIS QA install if users might migrate between installer technologies.

## Potential Pitfalls

- MSI cannot be cross-built from macOS/Linux with Tauri's WiX path; run it on Windows CI.
- Tauri's generated NSIS script includes migration logic for prior WiX installs, but the reverse NSIS-to-MSI migration needs to be verified.
- `bundle.targets: "all"` in shared config may produce unintended artifacts; release jobs should specify explicit bundles.
- Mac-only resources in shared `tauri.conf.json` currently require Windows placeholder workarounds. MSI work should not deepen that workaround.
- Unsigned MSI artifacts are acceptable for internal QA only. Public distribution needs a real Windows signing path.

## Sources

- Tauri Windows Installer: https://v2.tauri.app/distribute/windows-installer/
- Tauri Configuration Reference: https://v2.tauri.app/reference/config/
- Tauri Updater Plugin: https://v2.tauri.app/plugin/updater/
- Tauri Windows Code Signing: https://v2.tauri.app/distribute/sign/windows/
- Microsoft Windows Installer Best Practices: https://learn.microsoft.com/en-us/windows/win32/msi/windows-installer-best-practices
- Microsoft Code Signing Options: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options

## For Next Agent

The answer is "not really": oats has researched and implemented an NSIS-first Windows installer path, but not a dedicated MSI/WiX plan. The next useful step is a focused MSI spike on Windows CI/local Windows: add explicit `--bundles msi`, handle Windows-specific Tauri config/resources, verify artifact/signature generation, and test install/upgrade/uninstall behavior.
