# Windows Production Band-Aid Audit

Date: 2026-07-14

Branch: `codex/windows-msi-installer`

Comparison base: `315a16207a186f3bdad9a65af77961549f0f51ec` (`origin/main` merge base)

## Implementation Update

This report captures the branch before the cleanup that followed it. The
current worktree addresses BA-01, BA-02, BA-03, BA-04, BA-06, BA-08, BA-09,
BA-11, BA-12, BA-14, and BA-16, plus the documentation portion of BA-13. The
architectural follow-ups in BA-05, BA-07, BA-10, and BA-15 remain separate
work: installed-app CI coverage, macOS local-only diarization loading,
persistent/bounded notes inference, and immutable updater payload publication.
Wiring branded art into the public NSIS flow also remains from BA-13.

## Purpose

This report identifies code and release machinery that appears to have crossed the boundary from a development spike, local-machine workaround, or temporary smoke-test aid into a production path. It focuses on the Windows Local implementation, but includes adjacent macOS and release behavior where the new Windows work depends on it.

The findings below retain their original wording so the intent behind each
cleanup remains reviewable.

## Executive Summary

The named `windows-local-smoke.ps1` script is indeed development tooling, but it is not bundled into the installed app. Its problem is that it is presented as a smoke test while it only exercises a prebuilt sidecar against preinstalled models and caller-supplied fixtures. It does not prove installer, first-run download, Tauri integration, recording, updater, or uninstall behavior.

The more consequential production band-aids are:

1. The model publishing script can take whichever compatible-looking artifacts happen to exist on the publishing machine and publish them under the requested version.
2. The Windows notes runtime (`llama-cli.exe` and DLLs) is downloaded into a user-writable model directory and later executed, outside the signed installer's trust boundary.
3. Model versions, CDN roots, and runtime expectations are duplicated across scripts and Rust modules, allowing the publisher, host, and sidecar to drift independently.
4. Several failure paths silently substitute behavior: enabling the microphone, guessing platform capabilities, dropping failed diarization regions, or accepting stale readiness markers.
5. CI validates pieces of the implementation, but not the installed application or a clean first-run model download.

On 2026-07-14, all three configured Windows R2 manifests returned HTTP 200 and their raw SHA-256 values matched the pins in the host. The immediate links are therefore working now. The concern is not that today's objects are missing; it is that the process used to create, version, verify, and consume them is fragile.

No production code was found with a hard-coded `C:\Users\Michael\...` path.

## Priority Summary

| ID | Priority | Area | Finding | Suggested disposition |
|---|---|---|---|---|
| BA-01 | Blocker | Model publishing | Publisher can relabel arbitrary local artifacts as a requested version | Replace before public Local release |
| BA-02 | Blocker | Supply chain | Downloaded model bundle contains executable code later launched by the app | Move runtime into signed installer |
| BA-03 | High | Versioning | CDN roots and model versions have several independent sources of truth | Introduce one checked-in model lock/manifest |
| BA-04 | Medium | Smoke testing | Sidecar benchmark is named and documented as a product smoke test | Move/rename; add installed-app acceptance test |
| BA-05 | High | CI | Release confidence relies on manual and release-day validation | Add PR packaging and scheduled model/inference gates |
| BA-06 | Blocker | Transcription | Partial diarization failure can silently omit speech | Redesign transcript/diarization merge behavior |
| BA-07 | High | Offline contract | macOS sidecar can download diarization assets during execution | Require local-only model loading |
| BA-08 | High | Capabilities | Packaged-app IPC failures become optimistic user-agent guesses | Fail closed in production |
| BA-09 | High | Build tooling | Helpers can reuse stale output or accept an incomplete MSVC environment | Make builds fail immediately and validate inputs |
| BA-10 | High | Notes | Chunking repeatedly cold-starts the model and has no convergence bound | Use a persistent process and bounded reduction |
| BA-11 | Medium | Runtime controls | Smoke/debug environment variables form a hidden production control plane | Compile-gate tests; document and bound real tuning |
| BA-12 | High | Recording privacy | Unsupported source combinations silently turn the microphone on | Preserve user intent and reject invalid states |
| BA-13 | Medium | Docs/release metadata | Several docs and installer assets describe a different release state | Correct or explicitly supersede them |
| BA-14 | Medium | Generated files | Duplicate/local-feature schema output is committed without a production consumer | Remove duplicate output and standardize generation |
| BA-15 | High | Updater | Stable updater payloads are overwritten before `latest.json` is published | Publish immutable payloads, then switch manifest |
| BA-16 | Medium | Readiness | Repair ignores marker-removal errors and can leave a stale ready state | Treat non-NotFound errors as fatal; install atomically |

## Detailed Findings

### BA-01: The model publisher trusts and relabels local machine state

**Where:** `scripts/sync-windows-local-models.ps1`

The script starts from artifacts that have already been acquired on the operator's machine. Its speech, diarization, and notes discovery logic accepts the first matching directory from several layouts. That is convenient during a spike because it can reuse whatever a developer already downloaded, but the requested publication version is not proven to match the selected bytes.

For example, if a requested `v2` directory is absent while an unversioned or older compatible directory exists, the fallback may still package that directory under the `v2` destination. The notes lookup also accepts a generic `gemma-3-1b-it-q4_0.gguf` before an explicitly QAT-named artifact even though the product documentation promises the QAT model variant.

`Write-BundleManifest` hashes the files it found, which protects transport integrity after publication, but it records no authoritative upstream URL, Hugging Face revision, export command, build toolchain, license, or expected source hash. `-Force` can overwrite an allegedly immutable version prefix with different bytes.

**Why this looks like a development band-aid:** It optimizes for "publish the model files already on this workstation" instead of reproducibly constructing one reviewed release input.

**Production risk:** A release can be internally self-consistent yet contain the wrong model or runtime. Existing clients pinned to a version can receive different bytes after a forced replacement.

**Recommendation:** Add a checked-in Windows model lock containing exact upstream revisions, source hashes, export/build instructions, licenses, and expected bundle hashes. Acquire only those exact inputs. Remove source-layout fallbacks and prohibit mutation of an existing version key; repair should mean proving byte-for-byte equality, not replacing it.

### BA-02: The notes bundle downloads executable code outside the installer trust boundary

**Where:** `scripts/sync-windows-local-models.ps1`, `src-tauri/src/model_manager.rs`, and `src-tauri/ariso-stt/windows/src/notes.rs`

The notes bundle contains `llama-cli.exe`, dependent DLLs, and the GGUF model. The host downloads that bundle into the app's writable model directory, writes a completion marker, and the sidecar later launches `llama-cli.exe` from that directory.

The signed MSI/NSIS installer therefore does not fully describe the executable code the installed app will run. The initial manifest verification is useful, but readiness is subsequently represented by a marker and the executable is not rehashed immediately before each launch. A local process able to modify the user's model directory can replace the executable after installation.

**Why this looks like a development band-aid:** Packaging an upstream CLI beside a model is an expedient way to make a spike self-contained without integrating the native inference runtime into the product build.

**Production risk:** This expands the executable supply chain beyond installer signing and creates a persistent code-execution boundary in a user-writable directory.

**Recommendation:** Ship and sign the llama runtime and DLLs as installer resources/sidecars; download only model data. If a downloaded runtime must remain temporarily, pin every binary, verify Authenticode and hashes immediately before every launch, and install to an immutable versioned directory via atomic rename. Include licenses and provenance in the published bundle.

### BA-03: Model metadata has several manual sources of truth

**Where:** `scripts/sync-windows-local-models.ps1`, `src-tauri/src/model_manager.rs`, `src-tauri/ariso-stt/windows/src/models.rs`, and `src-tauri/ariso-stt/windows/Cargo.toml`

The established app/macOS model path uses one R2 host while Windows uses a second opaque host. Windows model versions are repeated in the publishing script, host model manager, and sidecar lookup logic. These values are related but not generated from one canonical artifact.

This allows a host release to request speech `v2` while the sidecar still searches for `v1`, or allows the publishing script to place a new bundle where no released host will look. The nested Windows sidecar crate also carries its own package version and is not included in the current release-please version update set.

**Why this looks like a development band-aid:** Independent constants were the fastest way to connect each new layer during the spike.

**Production risk:** A one-line version bump can produce a successful build whose first-run model install or inference fails in production.

**Recommendation:** Create one checked-in distribution manifest/lock consumed by the publisher, host, sidecar, tests, and release workflow. It should contain logical model IDs, versions, URLs, hashes, required files, runtime compatibility, and licenses. Generate language-specific constants when direct consumption is impractical.

### BA-04: `windows-local-smoke.ps1` is a sidecar benchmark, not an application smoke test

**Where:** `scripts/windows-local-smoke.ps1` and `scripts/README.md`

The script explicitly avoids downloads, Tauri, the installer, and updater. It defaults to a debug sidecar path, requires model directories and fixtures to exist already, synthesizes/repeats PCM input, lowers notes limits, and reports some semantically bad outcomes instead of failing the run.

This script is not bundled by Tauri and is not itself production runtime code. The band-aid is its role in the release story: its name and documentation imply end-to-end confidence that it cannot provide.

**Recommendation:** Move or rename it to something like `tools/bench/windows-local-sidecar.ps1` and describe it as a diagnostic/benchmark harness. Keep it useful for profiling. Add a separate clean-VM acceptance test that installs the actual artifact, launches it, downloads models through the UI, records microphone/system audio, transcribes, creates notes, retries offline, checks updater behavior, and uninstalls.

### BA-05: CI does not exercise the clean installed product path

**Where:** `.github/workflows/desktop.yaml`, `.github/workflows/release.yaml`, `scripts/README.md`, and ignored model-manager tests

Desktop CI builds the frontend and exercises Windows sidecar tests, but it does not run the full frontend test suite or the host Rust tests in the Windows job. Pull requests do not build an unsigned installer, so the signed release job becomes the first automated packaging test. The full Windows CDN download test is ignored and has no scheduled workflow caller. There is no real fixture-audio inference gate.

The scripts README also says desktop CI calls both sidecar and installer helpers, while the workflow only calls the sidecar helper.

**Why this looks like a development band-aid:** Local/manual checks cover the gaps while the branch is being assembled, but those checks do not travel with the release process.

**Production risk:** Broken resource paths, bundle contents, first-run downloads, installer configuration, and host-side integration can all merge before being exercised together.

**Recommendation:** Run frontend and host tests on Windows, build an unsigned installer on every relevant PR, add a cheap manifest/hash preflight, and run scheduled full-bundle plus real-inference tests. Preserve manual acceptance as a final check, not the primary gate.

### BA-06: Partial diarization failure can silently omit transcript content

**Where:** `src-tauri/ariso-stt/windows/src/models.rs` and `src-tauri/ariso-stt/windows/src/transcribe.rs`

The Windows sidecar treats diarization as optional at runtime even though host readiness defines the speech install as STT plus diarization. When diarization has no usable output, the code falls back to whole-audio single-speaker transcription. However, when diarization returns some segments, each segment is transcribed separately and short or failed clips are skipped. If any segment succeeds, missing gaps are not recovered by the whole-audio fallback.

Per-turn padding can also cause words near boundaries to be recognized more than once. The host receives syntactically valid JSON and can mark the recording complete without checking transcript coverage.

**Why this looks like a development band-aid:** Segment-by-segment ASR made it easy to attach speaker IDs without first designing a robust timestamp merge.

**Production risk:** A meeting can appear successfully transcribed while silently losing speech or duplicating boundary words.

**Recommendation:** Prefer one full-audio ASR pass with timestamps, then assign speakers by overlap. If the current segmented approach remains, define coverage invariants and fall back whenever any material region fails or remains uncovered. Return a structured degradation/error state rather than silently accepting partial output.

### BA-07: The macOS sidecar can download assets during an allegedly offline operation

**Where:** `src-tauri/ariso-stt/macos/Sources/ariso-stt/main.swift`

The macOS sidecar says that it never downloads models, yet its diarization setup calls `DiarizerModels.downloadIfNeeded`. This behavior predates the comparison base but was carried through the sidecar reorganization and now conflicts with the unified Local/offline contract.

**Why this matters to the Windows PR:** Windows model handling is being judged against macOS as the cleaner precedent. The macOS path is cleaner at the host boundary, but it still contains a hidden network-capable fallback inside inference.

**Recommendation:** Load only from host-managed local paths and fail into explicit repair/install UX when files are absent. Add a network-disabled Local acceptance test on both platforms.

### BA-08: Production capability failures become optimistic user-agent guesses

**Where:** `src/composables/usePlatformCapabilities.ts`

The composable has a browser/user-agent fallback intended for tests and browser development. It reports Local support for macOS and Windows. Any failure invoking the native capability command is caught and permanently replaced with that fallback.

**Why this looks like a development band-aid:** A browser preview remains usable before the Tauri command is available.

**Production risk:** A packaging, IPC, permissions, or initialization failure can be hidden as a fully supported platform. The UI then exposes actions that the native host may not be able to perform, obscuring the real fault.

**Recommendation:** Allow the user-agent fallback only in an explicitly detected non-Tauri development/test environment. In a packaged application, fail closed with an unavailable/error state and a retry path.

### BA-09: Build helpers can package stale output or accept a partial toolchain

**Where:** `scripts/build-windows-sidecar.ps1`, `scripts/import-windows-build-env.ps1`, and `scripts/build-windows-installers.ps1`

The sidecar builder does not immediately check Cargo's native exit code before copying the expected binary, so a failed compile can leave a previous binary available to package. The environment importer treats finding any `link.exe` as evidence that the native environment is complete and does not validate `cl.exe`, SDK paths, `INCLUDE`, or `LIB`. It also does not fail immediately on every native helper failure.

The installer helper computes normalized bundle choices for validation but later passes the raw bundle input. Signature checks scan broadly rather than proving the exact artifact selected for publication.

**Why this looks like a development band-aid:** These helpers accommodate whichever Visual Studio shell and prior build output happen to be present on the development machine.

**Production risk:** CI or a release operator can package stale code, fail late with confusing linker errors, or validate a different artifact from the one uploaded.

**Recommendation:** Check every native command's exit code, remove or quarantine stale outputs before building, validate the full MSVC/SDK environment, and pass one normalized artifact list through validation, signing, and publication.

### BA-10: Long-note generation repeatedly cold-loads the model and has no convergence bound

**Where:** `src-tauri/ariso-stt/windows/src/notes.rs` and `src-tauri/src/transcribe.rs`

Long transcripts are split and repeatedly reduced, but every chunk/group invocation starts a fresh `llama-cli.exe`, loading the model again. The reduction loop has no explicit maximum number of passes or proof that each pass is smaller. The host timeout was expanded to 30 minutes, which masks the architectural cost without guaranteeing completion.

**Why this looks like a development band-aid:** Chunk-and-reduce avoids the immediate context-window failure while preserving a simple one-shot CLI invocation.

**Production risk:** Ordinary long meetings can spend most of their time loading the model, hit the extended timeout, or continue reducing without meaningful progress on CPU-oriented Windows laptops.

**Recommendation:** Reuse a persistent model process/session, cap chunks and reduction passes, require measurable shrinkage, and return a specific error when the bound is exceeded. Benchmark 30-, 60-, and 120-minute transcripts on supported CPU/iGPU hardware.

### BA-11: Smoke and debug environment variables form a hidden production control plane

**Where:** `src-tauri/src/transcribe.rs`, `src-tauri/ariso-stt/windows/src/transcribe.rs`, and `src-tauri/ariso-stt/windows/src/notes.rs`

Release binaries honor environment variables for replacing the sidecar executable, selecting providers and thread counts, changing speaker/padding behavior, enabling debug output, and reducing note context/output limits. Some provider/thread tuning may be a legitimate operational escape hatch, but speaker, padding, executable replacement, and reduced-context controls primarily exist to make tests and smoke runs convenient.

Several values are weakly bounded or unbounded; for example, segment padding can be set without a meaningful maximum. These settings are also not represented in product configuration, support diagnostics, or release documentation.

**Why this looks like a development band-aid:** Process environment injection is quick and avoids designing a test seam or supported diagnostics interface.

**Production risk:** Environment inherited from a launcher, support script, or another tool can silently change transcript quality, model behavior, or executable selection.

**Recommendation:** Inject test executables through test-only constructors or compile-gated code. Keep only operational tuning that has a documented production purpose, enforce conservative bounds, surface active non-default tuning in diagnostics, and avoid honoring smoke-only variables in release builds.

### BA-12: Invalid recording settings silently enable the microphone

**Where:** `src/views/recordingSettings.ts`, `src/composables/useRecordingPermissions.ts`, and `src/composables/useRecorder.ts`

When a stored source combination is unsupported, normalization substitutes `{ microphone: true, systemAudio: false }`. The recorder repeats this substitution. The accompanying rationale references old settings snapshots, but Windows has no customer legacy state that must be preserved.

**Why this looks like a development band-aid:** It keeps recording from failing while capability work and settings migration are in motion.

**Production risk:** The app can activate a microphone the user did not select. This is a consent/privacy issue, not just a preference migration.

**Recommendation:** Define a clean capability-aware default for fresh installs, preserve explicit user choices, and reject impossible combinations with a visible settings error. Never silently turn on an audio source.

### BA-13: Documentation and installer metadata contain stale development states

**Where:** `README.md`, `CONTRIBUTING.md`, `.github/ISSUE_TEMPLATE/bug_report.yml`, `docs/superpowers/specs/2026-06-03-local-meeting-notes-design.md`, and `src-tauri/tauri.windows.conf.json`

Examples include:

- CONTRIBUTING still describes Visual Studio 2022 assumptions and says signing/updater work is missing, while the branch implements those paths.
- README support claims and download instructions do not consistently distinguish macOS DMG, Windows MSI, and Windows NSIS availability.
- The bug template includes Windows installer choices but still gives DMG-only version guidance and Console.app-only logs.
- The older Local design spec has updated paths but still describes sidecar downloads and a removed `download` command, making it a historical/current hybrid.
- Windows installer art is configured for WiX/MSI, while the public release path currently publishes NSIS. The committed bitmap is preferable to regenerating artwork at build time, but it must be attached to the installer users actually receive.

**Why this looks like a development band-aid:** Documentation was updated incrementally as the implementation direction changed, leaving intermediate states visible as current guidance.

**Recommendation:** Update the platform matrix, prerequisites, diagnostics, and release status together. Mark superseded specs clearly or refresh them. Configure the committed installer artwork for NSIS if NSIS remains the public target. Include the nested sidecar version in release automation.

### BA-14: Generated schema files include duplicate or local-feature output

**Where:** `src-tauri/gen/schemas/windows-schema.json`, `src-tauri/gen/schemas/desktop-schema.json`, and `src-tauri/gen/schemas/acl-manifests.json`

`windows-schema.json` is byte-for-byte identical to `desktop-schema.json` and has no distinct repository consumer. The ACL manifest's meaningful new entry is an empty `mcp` manifest produced by a local/debug feature set. The capabilities schema update itself is real and should remain.

**Why this looks like a development band-aid:** Running a local schema-generation command with the current machine's features produced files that were committed wholesale.

**Production risk:** Review noise hides meaningful permission changes, and future developers cannot tell which generated artifact is canonical.

**Recommendation:** Remove or ignore the duplicate Windows schema, regenerate ACL artifacts from the production feature set, and document one canonical generation command that CI can verify.

### BA-15: Updater publication overwrites stable payloads before switching the manifest

**Where:** `.github/scripts/release-publish.sh`

The updater uses stable, unversioned payload URLs. Publication overwrites those payload objects and only afterward uploads the new `latest.json`. A partial failure between those steps can leave the old signed manifest pointing at new bytes with a different signature/hash expectation.

**Why this looks like a development band-aid:** Stable object names make manual CDN management and client URLs simple during initial release work.

**Production risk:** An interrupted release can break updater checks/downloads for every installed client even though neither the old nor new release was intentionally withdrawn.

**Recommendation:** Upload payloads to immutable release-tag or content-hash keys, verify them, then publish `latest.json` as the final atomic pointer switch. Any convenience aliases should be updated afterward and should not be the signed updater targets.

### BA-16: Model repair ignores completion-marker removal failures

**Where:** `src-tauri/src/model_manager.rs`

The LLM repair path discards all errors while removing the `.complete` marker. That marker is also the readiness authority. If removal fails because the file is locked or permissions changed, repair can partially alter files and return an error while the stale marker still causes later status checks to report the model as ready.

**Why this looks like a development band-aid:** Ignoring cleanup errors makes repeated local development runs tolerant of files that are already absent.

**Production risk:** The UI can report a corrupt or incomplete model as installed after a failed repair.

**Recommendation:** Ignore only `NotFound`; abort repair on every other removal error. Prefer a versioned staging directory, verify all contents, and atomically rename it into place before writing the readiness marker.

## Unusual Constructs That Should Stay

Not every unfamiliar construct is a band-aid. The following have a clear production purpose:

- The unified `src-tauri/ariso-stt/{shared,macos,windows}` layout. It presents one product boundary while allowing native implementations and one executable name per platform.
- The Windows Tauri overlay that replaces macOS-only resources with the pinned llama.cpp installer resource.
- Committed BMP installer artwork. Keeping the source image in Git is simpler and more reproducible than regenerating it with a PowerShell drawing script. The remaining issue is wiring it to the public NSIS target.
- Platform-specific `.cmd` and shell sidecar stubs inside Rust test modules. They are test fixtures selected under `cfg(test)`, not shipped runtime fallbacks.
- The ignored full R2 integration tests. They are valuable because they exercise real large downloads; they need a scheduled workflow rather than deletion.
- The shared JSON schema/fixtures and the Windows sidecar `Cargo.lock`. These stabilize the cross-language contract and native dependency graph.
- Windows Job Object cleanup for child processes and 16 kHz decode/downmix handling. These are production hardening measures, not development accommodations.

## Recommended Cleanup Order

1. Fix the trust boundary first: BA-01, BA-02, BA-03, BA-15, and BA-16.
2. Fix correctness and consent before public Local testing: BA-06, BA-08, and BA-12.
3. Make long-running inference bounded and supportable: BA-10 and BA-11.
4. Turn the current diagnostics into honest tooling and add real gates: BA-04, BA-05, and BA-09.
5. Close the offline-contract and release-hygiene gaps: BA-07, BA-13, and BA-14.

The current Windows implementation contains substantial real product work, not merely a spike. The cleanup target is therefore not to remove Windows-specific code; it is to replace workstation-dependent acquisition, hidden fallbacks, and manual release assumptions with reproducible inputs and explicit failure states.
