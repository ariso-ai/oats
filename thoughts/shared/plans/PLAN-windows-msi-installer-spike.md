# Windows MSI Installer Implementation Plan

## Goal

Implement an internal Windows MSI artifact for oats using Tauri's built-in WiX/MSI bundler, while
keeping NSIS as the primary consumer installer.

The implemented end state is concrete:

- Windows release CI builds both NSIS and MSI from the same Windows sidecar build.
- CI uploads two internal artifacts:
  - `windows-nsis-internal`
  - `windows-msi-internal`
- Windows no longer needs a fake `src-tauri/binaries/mlx-swift_Cmlx.bundle` just to satisfy shared
  macOS bundle resources.
- MSI remains internal until signing, updater, install, upgrade, and uninstall behavior have been
  smoke tested.

## Confirmed Tauri Behavior

- `tauri build --bundles` accepts a space- or comma-separated list, so the Windows release helper
  can build both installer types in one pass:

  ```powershell
  powershell -ExecutionPolicy Bypass -File scripts\build-windows-installers.ps1
  ```

- Tauri automatically looks for and merges `src-tauri/tauri.windows.conf.json` with
  `src-tauri/tauri.conf.json`.
- Platform config merge follows JSON Merge Patch behavior, so setting a key to `null` in the
  Windows config removes that key from the merged config.
- Tauri supports Windows MSI through WiX Toolset v3 and NSIS through its setup EXE path. It does
  not require choosing only one.

## Files To Change

### `scripts/build-windows-sidecar.ps1`

Add a small CI/local helper so the duplicated Windows sidecar build logic lives in one place.

Expected behavior:

```powershell
$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

cargo "+stable-x86_64-pc-windows-msvc" build `
  --manifest-path src-tauri/ariso-stt-cross/Cargo.toml `
  --release `
  --locked `
  --target x86_64-pc-windows-msvc

New-Item -ItemType Directory -Force src-tauri/binaries | Out-Null
Copy-Item `
  src-tauri/ariso-stt-cross/target/x86_64-pc-windows-msvc/release/ariso-stt.exe `
  src-tauri/binaries/ariso-stt-x86_64-pc-windows-msvc.exe `
  -Force
```

Do not create `src-tauri/binaries/mlx-swift_Cmlx.bundle` in this script. The Windows config cleanup
below removes the need for that placeholder.

### `src-tauri/tauri.windows.conf.json`

Add Windows-specific Tauri config:

```json
{
  "bundle": {
    "resources": null
  }
}
```

Reason:

- The base config includes macOS-only `mlx-swift_Cmlx.bundle` resources.
- Windows currently papers over that with an empty placeholder directory in CI.
- Removing `bundle.resources` from the Windows merged config lets Windows packaging rely only on
  the target-named Windows sidecar binary.

Do not add custom WiX templates yet. The first implementation should use Tauri's default MSI.

### `.github/workflows/desktop.yaml`

Replace the inline Windows sidecar build block with the helper script.

Current Windows build block:

```yaml
- name: Build Windows ariso-stt sidecar
  if: runner.os == 'Windows'
  shell: pwsh
  run: |
    cargo build --manifest-path src-tauri/ariso-stt-cross/Cargo.toml --release --locked
    New-Item -ItemType Directory -Force src-tauri/binaries | Out-Null
    Copy-Item src-tauri/ariso-stt-cross/target/release/ariso-stt.exe src-tauri/binaries/ariso-stt-x86_64-pc-windows-msvc.exe
    New-Item -ItemType Directory -Force src-tauri/binaries/mlx-swift_Cmlx.bundle | Out-Null
```

Replace with:

```yaml
- name: Build Windows ariso-stt sidecar
  if: runner.os == 'Windows'
  shell: pwsh
  run: ./scripts/build-windows-sidecar.ps1
```

Do not add MSI packaging to PR validation yet. `desktop.yaml` should stay fast and validation-focused:
frontend build, sidecar tests, sidecar build, Rust build.

### `.github/workflows/release.yaml`

Rename the Windows package job from NSIS-only to installer-artifacts, then build both bundle types.

Change:

```yaml
package-windows:
  name: Build Windows NSIS artifact
```

To:

```yaml
package-windows:
  name: Build Windows installer artifacts
```

Replace the inline sidecar build block with:

```yaml
- name: Build Windows ariso-stt sidecar
  shell: pwsh
  run: ./scripts/build-windows-sidecar.ps1
```

Replace the NSIS-only build step:

```yaml
- name: Build Windows NSIS bundle
  env:
    TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
  run: npm run tauri:build -- --bundles nsis -- --features prod-api
```

With:

```yaml
- name: Build Windows installer bundles
  shell: pwsh
  env:
    TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
  run: ./scripts/build-windows-installers.ps1
```

Add an artifact assertion step immediately after the build:

```yaml
- name: Verify Windows installer artifacts
  shell: pwsh
  run: |
    $ErrorActionPreference = "Stop"

    $bundleRoot = "src-tauri/target/x86_64-pc-windows-msvc/release/bundle"
    $nsis = Get-ChildItem "$bundleRoot/nsis" -Filter *.exe -File
    $msi = Get-ChildItem "$bundleRoot/msi" -Filter *.msi -File
    $sigs = Get-ChildItem $bundleRoot -Recurse -Filter *.sig -File

    if ($nsis.Count -lt 1) { throw "Missing NSIS .exe artifact" }
    if ($msi.Count -lt 1) { throw "Missing MSI artifact" }
    if ($sigs.Count -lt 2) { throw "Expected updater signature artifacts for Windows installers" }

    $nsis | ForEach-Object { "NSIS: $($_.FullName)" }
    $msi | ForEach-Object { "MSI: $($_.FullName)" }
    $sigs | ForEach-Object { "SIG: $($_.FullName)" }
```

Keep the existing NSIS upload, then add a second upload:

```yaml
- name: Upload Windows MSI bundle
  uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a  # v7.0.1
  with:
    name: windows-msi-internal
    if-no-files-found: error
    compression-level: 0
    retention-days: 7
    path: |
      src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/**
```

Do not wire MSI into the public `publish` job yet.

### `docs/superpowers/specs/2026-06-27-windows-full-parity-design.md`

Update the build/release section to reflect the implemented packaging behavior:

- NSIS remains the primary public Windows installer target.
- MSI is built as an internal artifact for enterprise/admin validation.
- Public MSI release remains blocked on signing, updater verification, and install/upgrade/uninstall
  smoke tests.
- Windows config now removes macOS-only bundle resources instead of using a CI placeholder.

### `thoughts/shared/prs/173_description.md`

If this work is added to PR #173, update the PR description with:

- Added internal MSI artifact generation.
- Removed Windows MLX placeholder requirement through `tauri.windows.conf.json`.
- Added Windows sidecar build helper.
- Verification results for `nsis,msi` bundle generation.

## Implementation Order

1. Add `src-tauri/tauri.windows.conf.json` with `bundle.resources: null`.
2. Add `scripts/build-windows-sidecar.ps1`.
3. Update `.github/workflows/desktop.yaml` to call the helper and remove placeholder creation.
4. Update `.github/workflows/release.yaml`:
   - rename Windows package job
   - call the helper
   - build `nsis,msi`
   - verify artifacts
   - upload `windows-msi-internal`
5. Update the Windows full-parity design doc.
6. Run local Windows verification.
7. Commit and push to PR #173.
8. Watch the Windows CI/release job output for WiX/VBSCRIPT/MSI artifact failures.

## Local Verification Commands

Use `npm.cmd` on Windows to avoid PowerShell execution-policy problems with `npm.ps1`.

```powershell
npm.cmd test
npm.cmd run vite:build
cargo test --manifest-path src-tauri/ariso-stt-cross/Cargo.toml --locked
powershell -ExecutionPolicy Bypass -File scripts\build-windows-sidecar.ps1
powershell -ExecutionPolicy Bypass -File scripts\build-windows-installers.ps1
```

Then verify artifacts:

```powershell
Get-ChildItem src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis -Recurse
Get-ChildItem src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi -Recurse
Get-ChildItem src-tauri/target/x86_64-pc-windows-msvc/release/bundle -Recurse -Filter *.sig
```

Expected:

- At least one NSIS `.exe`.
- At least one MSI `.msi`.
- Updater `.sig` files for installer artifacts when `TAURI_SIGNING_PRIVATE_KEY` is set.

## Manual MSI Smoke Test

After a local or CI MSI is available:

```powershell
$Msi = (Get-ChildItem src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi -Filter *.msi | Select-Object -First 1).FullName
msiexec /i "$Msi" /L*v .codex-artifacts/msi-install.log
```

Manual checks:

- oats appears in Windows installed apps.
- oats launches from Start Menu.
- Settings opens.
- Local backend/model rows still render.
- The Windows sidecar exists in the installed app resources and can be spawned by the app.

Uninstall:

```powershell
msiexec /x "$Msi" /L*v .codex-artifacts/msi-uninstall.log
```

Manual checks:

- App uninstall completes.
- User data under the app data/model directories is not removed unless we intentionally add that
  behavior later.

## Upgrade/Migration Verification

Run these before any public MSI:

1. Install the current NSIS QA artifact.
2. Install the MSI artifact.
3. Record whether MSI:
   - replaces NSIS cleanly
   - installs side-by-side
   - blocks/conflicts
4. Install MSI v1.
5. Install MSI v2.
6. Confirm sidecar replacement by checking the installed `ariso-stt.exe` version/hash.
7. Confirm updater behavior with an MSI entry in a test `latest.json`.

Expected for this implementation:

- MSI artifact exists internally even if NSIS-to-MSI migration is not yet public-ready.
- Public docs continue to direct normal users to NSIS until migration/update behavior is proven.

## CI Acceptance Criteria

- `desktop.yaml` Windows validation passes without creating `mlx-swift_Cmlx.bundle`.
- `release.yaml` Windows package job produces:
  - `windows-nsis-internal`
  - `windows-msi-internal`
- Artifact verification step fails loudly if NSIS, MSI, or updater signatures are missing.
- The publish job remains unchanged and does not publish MSI publicly.

## Product Acceptance Criteria

- NSIS remains the default public Windows installer.
- MSI exists as an internal artifact suitable for enterprise/admin validation.
- No user-facing copy promises MSI publicly yet.
- No hand-authored WiX template is introduced.
- No Local model/runtime behavior changes are made as part of MSI packaging.

## Out Of Scope

- Replacing NSIS.
- Publishing MSI to R2 or GitHub Releases as a public download.
- Microsoft Store/MSIX packaging.
- Enterprise deployment docs beyond noting that MSI is internal.
- Custom WiX fragments/templates.
- Windows system audio, auto-record, model download, or tray fixes.

## References

- `thoughts/shared/handoffs/research-01-windows-msi-installer.md`
- Tauri Windows installer docs: https://v2.tauri.app/distribute/windows-installer/
- Tauri configuration docs: https://v2.tauri.app/develop/configuration-files/
- Tauri updater docs: https://v2.tauri.app/plugin/updater/
