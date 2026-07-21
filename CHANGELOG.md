# Changelog

## [0.17.1](https://github.com/ariso-ai/oats/compare/v0.17.0...v0.17.1) (2026-07-21)


### Bug Fixes

* clear the 'Upload failed' pill after a pending upload succeeds ([42aca65](https://github.com/ariso-ai/oats/commit/42aca65367a070fbd248983118101f1d619f2bee))
* do not show failed pill after successful upload ([3288608](https://github.com/ariso-ai/oats/commit/328860866d21a58d162cb14546028756434da7d7))
* preserve meeting id when resuming pending uploads ([2a97c2e](https://github.com/ariso-ai/oats/commit/2a97c2e9504d3d5cec30a8a67f843802e813927f))
* preserve meeting id when resuming pending uploads ([1949ba3](https://github.com/ariso-ai/oats/commit/1949ba3219268e572991a781ba6b075a118f8d1d))
* unblock macOS releases from Windows signing ([1fa3453](https://github.com/ariso-ai/oats/commit/1fa3453ee4bd063e149ce0e6fd2fbba571d54f7b))
* unblock macOS releases from Windows signing ([2e234a7](https://github.com/ariso-ai/oats/commit/2e234a794747fedd99333b3ebc911065a7fb58ca))

## [0.17.0](https://github.com/ariso-ai/oats/compare/v0.16.0...v0.17.0) (2026-07-19)


### Features

* add play button for buffered recordings in Pending Uploads ([1799613](https://github.com/ariso-ai/oats/commit/17996137c1217338d2b3f540ddd43d64747a91f3)), closes [#229](https://github.com/ariso-ai/oats/issues/229)
* harden Windows local parity and releases ([e9de46b](https://github.com/ariso-ai/oats/commit/e9de46b400c266b4f460bb569c0bd87fe7846e01))
* play button for buffered recordings in Pending Uploads ([229b490](https://github.com/ariso-ai/oats/commit/229b490d256bd3e4b2737e5e8cca75684e5f8ff2))
* run the OAuth handshake in the default browser via a loopback c… ([f816224](https://github.com/ariso-ai/oats/commit/f816224aeeb8d12829d99f340edd9f74f07c5326))
* run the OAuth handshake in the default browser via a loopback callback ([ea3cd55](https://github.com/ariso-ai/oats/commit/ea3cd5587c9476a16dceccfecedfd530d7781f3a))
* treat the next meeting's start as the stop-prompt transition point ([4ab1d41](https://github.com/ariso-ai/oats/commit/4ab1d41a15d7132935a1b072ec0a4ab85d0bb9fd)), closes [#230](https://github.com/ariso-ai/oats/issues/230)
* **windows:** add native audio parity ([2835acd](https://github.com/ariso-ai/oats/commit/2835acdb65cf2ee378784d09df39e15c495e757a))


### Bug Fixes

* adds required permission ([a6a4667](https://github.com/ariso-ai/oats/commit/a6a466705b74b0143638b8fb11aa0e761bc7ea03))
* **ci:** scope MLX resource to macOS ([17798ca](https://github.com/ariso-ai/oats/commit/17798cac10f33d6df41dad72a71c683a807717eb))
* harden Windows local production paths ([1e564d0](https://github.com/ariso-ai/oats/commit/1e564d09465674cae5de501d3de8a2cc5792e637))
* match settings titlebar background ([071d900](https://github.com/ariso-ai/oats/commit/071d900acc756683ae62555a5b939eed969d70c7))
* prompt to stop recording when the next calendar meeting starts ([6ba6fa6](https://github.com/ariso-ai/oats/commit/6ba6fa6ee5accbd38d1a36efcbd3ad9d54b8cd05))
* **recording:** preserve recorder lifecycle and titles ([5554367](https://github.com/ariso-ai/oats/commit/55543679521989f4bf4c144d03f00ae39f9a1242))
* **recording:** unify Windows launch and refresh flows ([fd307f5](https://github.com/ariso-ai/oats/commit/fd307f5bbc5f5021a7394cb15022723acd01b81d))
* repair autofix regressions in play-button a11y change ([3aa332c](https://github.com/ariso-ai/oats/commit/3aa332cae513877d9c1fcb880205c6f2ce527ef9))
* **updater:** publish immutable payloads ([f56b48f](https://github.com/ariso-ai/oats/commit/f56b48f357607d69558e9ea90c98ac9edc74bbb6))
* **windows:** harden audio capture lifecycle ([2ad04c5](https://github.com/ariso-ai/oats/commit/2ad04c52b4edca113d2d9d8a1872e19bf19b7cf0))
* **windows:** hide local inference consoles ([73395ce](https://github.com/ariso-ai/oats/commit/73395cec3dd08c601dd77d0edf3cac2e07a79677))

## [0.16.0](https://github.com/ariso-ai/oats/compare/v0.15.0...v0.16.0) (2026-07-08)


### Features

* **local:** force-new recording seam in finalize/start-window ([#174](https://github.com/ariso-ai/oats/issues/174)-followup) ([e4c91fc](https://github.com/ariso-ai/oats/commit/e4c91fc74727f45207752bc925d7185144fe189e))
* **local:** regenerate default note title from AI notes ([#208](https://github.com/ariso-ai/oats/issues/208)) ([fe35395](https://github.com/ariso-ai/oats/commit/fe35395662175c9eb0df670984e29002f1219101))
* **recording:** empty detail starts fresh; wire forceNew seam (both backends) ([1c0be7b](https://github.com/ariso-ai/oats/commit/1c0be7b0a181d8efebeb30054d73f63842047009))


### Bug Fixes

* **recording:** empty detail pane starts a fresh recording (both backends) ([cda7cfb](https://github.com/ariso-ai/oats/commit/cda7cfb6c595f7304e79484317bd2ca871388aed))
* **recording:** open_meeting_picker takes only defaultMeetingId ([b12444f](https://github.com/ariso-ai/oats/commit/b12444fd82845d07630834dd76c6ab96a92d7c5b))
* **search:** time-box cmd-K search so it can't hang ([380a4c8](https://github.com/ariso-ai/oats/commit/380a4c860e616ab523609ac61e374a2355be3caf))
* share agent instructions through symlinks ([63ddbee](https://github.com/ariso-ai/oats/commit/63ddbee4ad96e64cb0b26abb7d9ee20b2130569e))

## [0.15.0](https://github.com/ariso-ai/oats/compare/v0.14.0...v0.15.0) (2026-07-07)


### Features

* configurable Obsidian vault directory for local backend ([#200](https://github.com/ariso-ai/oats/issues/200)) ([30a7103](https://github.com/ariso-ai/oats/commit/30a71032abea5c5a5301e0f1a685bbe1dc9d514c))
* continue an existing meeting when Start recording is clicked with a meeting open ([eea4353](https://github.com/ariso-ai/oats/commit/eea435348f6319aa80a584685556af8016e9abaa))
* **library:** decideStartRecording helper for new-vs-continue branching ([8c3848f](https://github.com/ariso-ai/oats/commit/8c3848f59604e68d61ea4b1ea5ed16a8d678bb5f))
* **library:** New-vs-Continue choice dialog + promise driver ([2efe069](https://github.com/ariso-ai/oats/commit/2efe0693b0fd71d0dc2daca0989d9740b0aef8e5))
* **library:** prompt New-vs-Continue when a meeting is open on Start ([eccacd0](https://github.com/ariso-ai/oats/commit/eccacd08710926e6f80e35931ed88023e84d4b9d))
* local-backend Obsidian vault for notes + audio ([e0d231d](https://github.com/ariso-ai/oats/commit/e0d231d2dcfc4f51ee0ead6a7dc3370511714933))
* **local:** force-append seam in finalize_core for continue-recording ([133ca41](https://github.com/ariso-ai/oats/commit/133ca41333fc465a4f1075a3db016079c685b74b))
* **local:** thread localAppendId from waveform window into finalize ([c6a137c](https://github.com/ariso-ai/oats/commit/c6a137ca5df9649d2a03ba82145367b9437e1909))
* **picker:** feature the open meeting as the default choice ([64dc333](https://github.com/ariso-ai/oats/commit/64dc33363fa61c9052bf3f615f9e4d964809b50c))
* **recorder:** thread localAppendId into the waveform window URL ([41fec5a](https://github.com/ariso-ai/oats/commit/41fec5a408501e5935604e3b5506d6809b76e0bd))
* **settings:** front-truncate vault path under 20 chars ([a1b12c7](https://github.com/ariso-ai/oats/commit/a1b12c7cd4751c9e80566f8ccb19e454cd79bcd5))
* **settings:** refine vault-location UX ([00ee9f8](https://github.com/ariso-ai/oats/commit/00ee9f8fe831815d6c3dda32aeae07ff25c203e5))
* **vault:** add audio_file and notes_written to RecordingMeta ([d36f71d](https://github.com/ariso-ai/oats/commit/d36f71d2bdff9e51bba4a113782435b6fc871329))
* **vault:** add dialog plugin + vault dir tauri wrappers ([9f8c0d0](https://github.com/ariso-ai/oats/commit/9f8c0d0cfd2b1060f9e2c4d33fff6d406626b29b))
* **vault:** audio attachment read/write ([00ae951](https://github.com/ariso-ai/oats/commit/00ae951bd7c69c82910a372963d91cca18432770))
* **vault:** configurable vault root override + .oats meta dir ([8ca2249](https://github.com/ariso-ai/oats/commit/8ca2249b6900e0fcec88b5c929f1ed539eb4bc3c))
* **vault:** create the vault at startup ([acf56d9](https://github.com/ariso-ai/oats/commit/acf56d9d1444e99153afaad0eb24f1cc6b41a919))
* **vault:** get_vault_dir / set_vault_dir commands ([b65aa61](https://github.com/ariso-ai/oats/commit/b65aa61b16831dbe8ffd091b724a04cd4ff10f2b))
* **vault:** has_note/has_audio reflect vault artifacts ([a029dc6](https://github.com/ariso-ai/oats/commit/a029dc6a47955c9e3d3f9ae59e093ef13a76cfff))
* **vault:** note vault location and sync privacy in Settings ([8142f11](https://github.com/ariso-ai/oats/commit/8142f11916624cc00a65b9289706fed6718bc6ff))
* **vault:** note_basename and collision-safe unique_basename ([25c8bdc](https://github.com/ariso-ai/oats/commit/25c8bdc113a7a47cc9891cd7ca8146e108de695c))
* **vault:** notes regeneration replaces the vault note ([200ebc8](https://github.com/ariso-ai/oats/commit/200ebc8fdc3943bb4805b34cbeae7cbde5d16755))
* **vault:** one-time legacy-recordings migration + startup override wiring ([47ec4a0](https://github.com/ariso-ai/oats/commit/47ec4a03b063ba3572d2b0ab0cc00f0ca43791b2))
* **vault:** play audio from the vault with legacy fallback ([8844f7a](https://github.com/ariso-ai/oats/commit/8844f7a96c5da44899c8763e30749185f0781b76))
* **vault:** read/open notes from the vault with legacy fallback ([0730d95](https://github.com/ariso-ai/oats/commit/0730d95117abb03bead7c50c178fa9b1a371384c))
* **vault:** render_note and note_body ([154a426](https://github.com/ariso-ai/oats/commit/154a4265a66616a8673ace345318027c1c9c40fa))
* **vault:** resolve local recordings under the vault's .oats dir ([cb6f15f](https://github.com/ariso-ai/oats/commit/cb6f15f9b914ebdec7fc9c5d2f79276165ab5582))
* **vault:** scan_vault, find_note, read_note by oats_id ([8f95341](https://github.com/ariso-ai/oats/commit/8f953419e3a3d5b06f3751be4714c03124e74a05))
* **vault:** Settings vault-location control + Library refresh on change ([568b9b9](https://github.com/ariso-ai/oats/commit/568b9b914b7abe52b1309985afc92ed9eb5aeb19))
* **vault:** store local recording audio only in the vault ([6c5fc5c](https://github.com/ariso-ai/oats/commit/6c5fc5cf02812b6ca0daf168d8c09fd71ae4f028))
* **vault:** vault root and ensure_vault bootstrap ([952252f](https://github.com/ariso-ai/oats/commit/952252f7dbebced8ef05d41295161d157b1a2222))
* **vault:** write generated notes into the vault ([c2cee38](https://github.com/ariso-ai/oats/commit/c2cee382fd3cf28f2fbc277c89a41502e67741c7))
* **vault:** write_note and delete_recording_artifacts cascade ([7d2392b](https://github.com/ariso-ai/oats/commit/7d2392b82ae3fb01ecbcc7cd4d298b7da26ebb65))


### Bug Fixes

* **ci:** stop CodeQL Swift from starving the runner + skip no-Swift runs ([f5754fd](https://github.com/ariso-ai/oats/commit/f5754fd9281f7d38425e86ace51c5c19f8dfc313))
* **ci:** stop CodeQL Swift from starving the runner + skip no-Swift runs ([af000e7](https://github.com/ariso-ai/oats/commit/af000e771ab377484e517262c32aa4a7f6d4528e))
* close open meeting when switching backend ([f8b1e3b](https://github.com/ariso-ai/oats/commit/f8b1e3bd216bcc370457a7c0a7cdbba4ce700a51))
* **deps:** adapt to tungstenite 0.29 Utf8Bytes Message API ([a246787](https://github.com/ariso-ai/oats/commit/a246787a0d3f2f4df0ca940d87a9a0b53a2b6f7b))
* **library:** correct attendees popover a11y semantics ([27bbde8](https://github.com/ariso-ai/oats/commit/27bbde8ff5f50ae82a6b223526fa0d6eb1c2f7f1))
* **library:** relabel choice dialog buttons + pointer cursor on hover ([a5eb1e7](https://github.com/ariso-ai/oats/commit/a5eb1e7833141baf793b218c6c2a6b90bfdc10ab))
* **library:** show attendees dropdown in meeting detail ([99a9715](https://github.com/ariso-ai/oats/commit/99a9715426388ace870cfb13e8999af005111997))
* **library:** show attendees dropdown in meeting detail ([f62aa2d](https://github.com/ariso-ai/oats/commit/f62aa2dca9f2e8fdfe8641fc7633eb66bc9630d0)), closes [#143](https://github.com/ariso-ai/oats/issues/143)
* **local:** validate forced append-target id before path join ([e282aa8](https://github.com/ariso-ai/oats/commit/e282aa8d712717e490dacea8f0750bd8aa0baa28))
* **menu:** open Settings window from the app menu ([daeea65](https://github.com/ariso-ai/oats/commit/daeea6586ef151983c3fee2134d7d89ea3f1073f))
* **menu:** open Settings window from the app menu ([e4fb6a7](https://github.com/ariso-ai/oats/commit/e4fb6a76d52bfc6a7ed7afbbfad034216bf6b33d)), closes [#212](https://github.com/ariso-ai/oats/issues/212)
* **notes:** migrate to @tiptap/markdown and fix list/checkbox rendering ([4104f95](https://github.com/ariso-ai/oats/commit/4104f9592f22d36c463ce65dd80f248b776e2fef))
* **notes:** migrate to @tiptap/markdown and fix list/checkbox rendering ([ddc5cad](https://github.com/ariso-ai/oats/commit/ddc5cad2b9363c7ece0d7dd48f64627424d20705))
* **picker:** shared View-all list + real forced-default fallback; reject id 0 ([a0dd738](https://github.com/ariso-ai/oats/commit/a0dd738fe84ff9dbc7287f46de01b2c37d24f54f))
* propagate in-app rename to the vault note + attachment ([bc33230](https://github.com/ariso-ai/oats/commit/bc332306d21393e99546ddd578fc872fc830c88b))
* reset meeting in detail section when switching backend ([e13827f](https://github.com/ariso-ai/oats/commit/e13827f66392566d76dc79f6db330a5a8ecf4b22))
* **vault:** gate test-only clear_vault_override behind #[cfg(test)] ([aacfaf0](https://github.com/ariso-ai/oats/commit/aacfaf06c7f6b826518a283ced6d478e7d87e387))
* **vault:** has_audio reflects vault attachment existence ([dc35fe4](https://github.com/ariso-ai/oats/commit/dc35fe47307be634971d91e91a5cb45df8bbd453))
* **vault:** keep failed clips durable when vault creation fails ([e45e5fc](https://github.com/ariso-ai/oats/commit/e45e5fcb4ce67a95616ed95844b4d5b2c8417cce))
* **vault:** propagate in-app rename to the vault note and attachment ([4425246](https://github.com/ariso-ai/oats/commit/4425246e2c502c0b1522cc7bc6c1ff47e63e1d07))
* **vault:** restore note in place when attachment rename fails ([552433b](https://github.com/ariso-ai/oats/commit/552433bdbaf1814de580f16061d3eb0d3a8a2196))
* **vault:** scope dialog capability to settings window; path overflow + test-mock hygiene ([1251a2e](https://github.com/ariso-ai/oats/commit/1251a2e8e007b4ee62f889eb3dace0cbd1a6addc))

## [0.14.0](https://github.com/ariso-ai/oats/compare/v0.13.0...v0.14.0) (2026-07-03)


### Features

* local (offline) multi-recording — append resumed clips to the recent recording ([1cbf40a](https://github.com/ariso-ai/oats/commit/1cbf40abf46f22704e281aed4daf29169bf638ed))
* **local:** 5-min append-window decision ([d8f1402](https://github.com/ariso-ai/oats/commit/d8f1402c81c23b007b5c16efa9a141f38b642a46))
* **local:** append a resumed clip to the recent recording ([0bfc83d](https://github.com/ariso-ai/oats/commit/0bfc83db30262e733e74f44cb0e5bb862527b6b4))
* **local:** persist structured segments.json per recording ([cf3ea77](https://github.com/ariso-ai/oats/commit/cf3ea77a5f7c9f6ef0098dcb684bad477e1c937a))
* **local:** pure offset helpers for stitching clips ([8a276af](https://github.com/ariso-ai/oats/commit/8a276afa21761a5607ab1c9e3a650a8464b6c84c))


### Bug Fixes

* **local:** crash-safe append ordering + explicit failed-clip save + clear notes_error ([3f60ddc](https://github.com/ariso-ai/oats/commit/3f60ddcd09119f7a71380c9bfeed86ad6ba5d707))
* **local:** discard superseded notes output on append ([01b15f8](https://github.com/ariso-ai/oats/commit/01b15f88a80e56187ebf9db805c9b1daf87c2e2b))
* **local:** dock resume to the current meeting instead of a phantom new note ([b432bd6](https://github.com/ariso-ai/oats/commit/b432bd6d5dc6e604ffeb46ddffb6df912c09aed8))
* **local:** fall back to fresh recording when target unreadable + doc clarifications ([7e30577](https://github.com/ariso-ai/oats/commit/7e305770dc2a38f1d874674e63ea041566157ace))

## [0.13.0](https://github.com/ariso-ai/oats/compare/v0.12.0...v0.13.0) (2026-07-02)


### Features

* clearer editable-name affordances in MeetingDetailView ([b202710](https://github.com/ariso-ai/oats/commit/b202710a1052385dfaff90edb8c00aec130d3017))
* clearer editable-name affordances in MeetingDetailView ([b76435d](https://github.com/ariso-ai/oats/commit/b76435d5c077b81a45a76f44d5894f4a570f53b7))
* cloud multi-recording support in oats (stacked clips, per-clip transcript + delete) ([1f0b409](https://github.com/ariso-ai/oats/commit/1f0b409a9dba591f8581de9fb4c43694ddad104b))


### Bug Fixes

* activate clip rows on Space + aria-pressed (CodeRabbit) ([525997e](https://github.com/ariso-ai/oats/commit/525997e4835e976a48e228bc8d4e8b781c1c3632))
* **deps:** enable reqwest "query" feature for 0.13 ([a173c93](https://github.com/ariso-ai/oats/commit/a173c93d3a48b257c1a1f5192aa4d304fb839fc7))
* **deps:** unify on rustls + ring TLS under reqwest 0.13 ([5b92285](https://github.com/ariso-ai/oats/commit/5b92285abbe8ad1d310a4784d4e5eae7e014a6fc))
* visible keyboard-focus ring on editable titles (CodeRabbit) ([196e49e](https://github.com/ariso-ai/oats/commit/196e49e66d564824f5df32f42eb0b7e36b2f061d))

## [0.12.0](https://github.com/ariso-ai/oats/compare/v0.11.0...v0.12.0) (2026-06-30)


### Features

* check for updates every 2h while running ([d23f789](https://github.com/ariso-ai/oats/commit/d23f7897408337d0c3ca708e5cd03b6a8b4c6349))
* check for updates every 2h while running ([5133f23](https://github.com/ariso-ai/oats/commit/5133f2313921ea88ea35f60a64b96f6f4744f52c))

## [0.11.0](https://github.com/ariso-ai/oats/compare/v0.10.0...v0.11.0) (2026-06-30)


### Features

* Meeting stop reminder notification setting (on by default) ([3d11e65](https://github.com/ariso-ai/oats/commit/3d11e655c3dd275d463ef68c4639fbd4b9bdc35f))
* Meeting stop reminder notification setting (on by default) ([88e0f7f](https://github.com/ariso-ai/oats/commit/88e0f7fd3ba1f922c6a8eed852f4d53e9fcd76dd))
* meeting-end prompt window + commands ([#157](https://github.com/ariso-ai/oats/issues/157)) ([3560b10](https://github.com/ariso-ai/oats/commit/3560b1049a35f291b0d7cfd37417f57079ae8d5b))
* meeting-end stop prompt for back-to-back calls ([#157](https://github.com/ariso-ai/oats/issues/157)) ([c63ece5](https://github.com/ariso-ai/oats/commit/c63ece570757be0275b1543d612ae137c85c8f20))
* meeting-end stop watch wiring in WaveformView ([#157](https://github.com/ariso-ai/oats/issues/157)) ([f08e9ec](https://github.com/ariso-ai/oats/commit/f08e9ecc0f0f5c2a55a73a7125c6ae7e32c36a34))
* meeting-end watch pure helpers ([#157](https://github.com/ariso-ai/oats/issues/157)) ([4f6774f](https://github.com/ariso-ai/oats/commit/4f6774f8fdcb47d7ad2bf377c02aa6688cc8f2ee))
* MeetingEndPromptView + route ([#157](https://github.com/ariso-ai/oats/issues/157)) ([b7b68e7](https://github.com/ariso-ai/oats/commit/b7b68e754257f465f587fb54dd8705500474440b))
* native microphone capture via Core Audio input (no voice-processing duck) ([aed1d6b](https://github.com/ariso-ai/oats/commit/aed1d6b229258c912472006849d0129f0d7561f9))
* native microphone permission (AVCaptureDevice) ([1bc7ea9](https://github.com/ariso-ai/oats/commit/1bc7ea95c8f2f676f9c26ff8e02d191023e0eb9d))
* native microphone permission + capture wrappers (frontend) ([f646477](https://github.com/ariso-ai/oats/commit/f646477884780f74ad2f25f451b243ad2d60fb4c))
* parseMeetingEndPromptParams ([#157](https://github.com/ariso-ai/oats/issues/157)) ([5b636f0](https://github.com/ariso-ai/oats/commit/5b636f03b42965222613b4d8e2eeee2f78c8c522))
* register microphone capture + permission commands ([d7dade9](https://github.com/ariso-ai/oats/commit/d7dade9ca2a17ff75d0c9be9242bacd07657d008))
* request_mic_monitor_rearm to re-arm after meeting-end stop ([#157](https://github.com/ariso-ai/oats/issues/157)) ([6b217ec](https://github.com/ariso-ai/oats/commit/6b217ec8a97305a7518949eb64c0d39d32e0348a))
* source microphone from native capture instead of getUserMedia (issue [#159](https://github.com/ariso-ai/oats/issues/159)) ([703408e](https://github.com/ariso-ai/oats/commit/703408efd49954304068bb9795e1e19e7fa924b5))


### Bug Fixes

* **#159:** native mic capture to eliminate startup system-audio ducking ([bc944eb](https://github.com/ariso-ai/oats/commit/bc944ebf22373bd3728c6193b85091ef737d3283))
* accept non-interleaved mono mic format (issue [#159](https://github.com/ariso-ai/oats/issues/159)) ([beff37f](https://github.com/ariso-ai/oats/commit/beff37f353cd7496bbb8563d6cee9d328f030f78))
* gate meeting-end lookup on reminder setting, not just the timer ([3256b39](https://github.com/ariso-ai/oats/commit/3256b392e1dee0ef6e6c82000d7cbd656038cb57))

## [0.10.0](https://github.com/ariso-ai/oats/compare/v0.9.0...v0.10.0) (2026-06-21)


### Features

* new setting option for silence detection ([8cc68a1](https://github.com/ariso-ai/oats/commit/8cc68a1cbae3285b59a1d800286bec384314007c))
* replace silence notification with an in-app prompt window ([b92b2b7](https://github.com/ariso-ai/oats/commit/b92b2b7252dab7ccd9f1d278da4492d263b65b23))
* replace silence notification with an in-app prompt window ([2ee2f32](https://github.com/ariso-ai/oats/commit/2ee2f32216f4defb1f189f7ca7ac650cd845f9c6))
* verify on-device STT model downloads from a pinned R2 mirror ([15c755f](https://github.com/ariso-ai/oats/commit/15c755ff0a778092355973b5d531b1ddf88cd4c9))
* verify on-device STT model downloads from a pinned R2 mirror ([c62dc0b](https://github.com/ariso-ai/oats/commit/c62dc0b528978e25a5cc6940e15ae752e8a78626))


### Bug Fixes

* guard null device UID and validate tap format (GHSA-cvf3-62r6-ch7v) ([b80a621](https://github.com/ariso-ai/oats/commit/b80a621e3b00f685de220b6898819317e48c410a))
* guard null device UID and validate tap format in system-audio capture ([b64a966](https://github.com/ariso-ai/oats/commit/b64a966b86df5a3b0aa7efa79b679728e37ee25d))
* publish Homebrew cask checksum via PR, point cask at R2 ([f682c93](https://github.com/ariso-ai/oats/commit/f682c9330ee32fd44b66a6f8d485e0d242b89a34))
* publish Homebrew cask checksum via PR, point cask at R2 ([1e10ef9](https://github.com/ariso-ai/oats/commit/1e10ef9bedf327df7b04d09dbac9fa6d5896c236)), closes [#147](https://github.com/ariso-ai/oats/issues/147)
* verify on-device LLM model downloads with pinned SHA-256 ([c4af2d8](https://github.com/ariso-ai/oats/commit/c4af2d8e36875231c25f1cb6cf56c6f074c75b3a))

## [0.9.0](https://github.com/ariso-ai/oats/compare/v0.8.1...v0.9.0) (2026-06-19)


### Features

* add Ari-join confirm composable and dialog ([64cc999](https://github.com/ariso-ai/oats/commit/64cc999a1b69936f06c3bf4f62f087fa3b73aa3f))
* add arisoTruthy and shouldConfirmAriJoin helpers ([b1bc708](https://github.com/ariso-ai/oats/commit/b1bc708981d15c6c3de8d8f8c144ed9a56ab3c2c))
* confirm Ari auto-join before recording from the library ([098cf09](https://github.com/ariso-ai/oats/commit/098cf09c29e2dcbc7bb0585ccc1fbad953a0d041))
* confirm Ari auto-join before recording from the meeting picker ([4b4e0b1](https://github.com/ariso-ai/oats/commit/4b4e0b1b4901eb81858fb873f6e328a714b89201))
* meeting prompt corner dismiss + Take notes split menu ([4cbcf81](https://github.com/ariso-ai/oats/commit/4cbcf81ad5c4cc443475aa92a8bf403914e81edc))
* meeting-prompt URL-query parser ([25909e8](https://github.com/ariso-ai/oats/commit/25909e8634d789cb3bfa8c56bc31321d9de3a927))
* meeting-start notification view + route ([41b3dd8](https://github.com/ariso-ai/oats/commit/41b3dd8597eb586d373ed752cf3498cd7a03d173))
* meeting-start notification with countdown bar ([#121](https://github.com/ariso-ai/oats/issues/121)) ([61eafcf](https://github.com/ariso-ai/oats/commit/61eafcfce32d85f1b63683d937063c367ea609ad))
* open custom meeting-start notification window in place of the UNC prompt ([0196509](https://github.com/ariso-ai/oats/commit/01965091d2505d5a4d79f51ba850d93880f5b88d))
* plumb auto_join_scheduled flag into frontend meeting types ([7c2a29b](https://github.com/ariso-ai/oats/commit/7c2a29b51addc277c0c2ebd1bf6a8196ad39dd65))
* polish meeting-start notification (icon, dismiss, live title) ([a2138f3](https://github.com/ariso-ai/oats/commit/a2138f31400528fe7b608c1dc1e755d19867938d))
* show 'Ari will join' label on attendee lines ([19a4272](https://github.com/ariso-ai/oats/commit/19a4272dfcf9b7fa46eae78ee70bc9fcde26f009))


### Bug Fixes

* add checksum-backed Homebrew cask install ([abc974a](https://github.com/ariso-ai/oats/commit/abc974ae2ef04db2236789d3565d9c2a5336fd4f))
* align meeting prompt view with the app design system ([0dc7b8d](https://github.com/ariso-ai/oats/commit/0dc7b8d0d377e7830d8614232ea3adcc559bf414))
* align meeting prompt view with the app design system ([968e6fb](https://github.com/ariso-ai/oats/commit/968e6fb8f78a4db66405aa46be49adf7763463d9))
* corner dismiss as a bordered circle straddling the card corner ([366abc3](https://github.com/ariso-ai/oats/commit/366abc3aa8b3dfc751326a51164b7e6d03915869))
* dismiss as a secondary pill button + rectangular corner close ([8140e0f](https://github.com/ariso-ai/oats/commit/8140e0fd745e9ed787f2fe07f3060bdabe2ec8fe))
* enlarge corner dismiss and match Dismiss width to Take notes ([50c6623](https://github.com/ariso-ai/oats/commit/50c6623c9837488b8572d714189265c7a360f90e))
* make appdmg optional for installs ([0ce228c](https://github.com/ariso-ai/oats/commit/0ce228c84bdfe98bb9b0bffbdc3e199600559ceb))
* make enabledPlugins a record to match settings schema ([11a49c9](https://github.com/ariso-ai/oats/commit/11a49c9dffdc8125d0eba2f29868b4ee29c0cab7))
* make enabledPlugins a record to match settings schema ([53f604c](https://github.com/ariso-ai/oats/commit/53f604c459c743b8703f9add214d5e62eb43c839))
* make meeting prompt fill its window and clip the countdown bar ([0c517ac](https://github.com/ariso-ai/oats/commit/0c517ac2b67f52b98a101034aedb23b48899672c))
* notarize composed dmg ([8e3b08e](https://github.com/ariso-ai/oats/commit/8e3b08e24f5645a5d9a5c3b689d641976bb6ce0a))
* **recorder:** derive elapsed time from wall-clock, not timer ticks ([6aadc54](https://github.com/ariso-ai/oats/commit/6aadc54559991f68c948aea6bff6dab7876bc70c))
* shorten meeting prompt window with symmetric vertical padding ([f126a39](https://github.com/ariso-ai/oats/commit/f126a3904b27881cc90329a767b1da25150aa373))
* show Google avatar in Settings account row ([4024c59](https://github.com/ariso-ai/oats/commit/4024c59303c1c29985d1e049ce0001c3d852dc9f))
* tighten meeting prompt dismiss corner and dropdown placement ([007db3e](https://github.com/ariso-ai/oats/commit/007db3e2880aaa3aa8f73147d8e3c39c98a4c1c1))
* UI tweaks — title width, transcript scrollbar, hide local front-matter ([7317f50](https://github.com/ariso-ai/oats/commit/7317f5045a4db144f9653e17b7b496288ce2a5ef))
* widen editable title, slim transcript scrollbar, hide local front-matter ([182452a](https://github.com/ariso-ai/oats/commit/182452a9c864304c660738c1063d063797144890))

## [0.8.1](https://github.com/ariso-ai/oats/compare/v0.8.0...v0.8.1) (2026-06-18)


### Bug Fixes

* hide model banner on unsupported platforms ([ca70fad](https://github.com/ariso-ai/oats/commit/ca70faddc0bb164f60517bdeba08dc2178f2ad69))
* run release-publish.sh with modern bash on hosted runner ([b2b0100](https://github.com/ariso-ai/oats/commit/b2b0100934256dd18c4fb9a1a82379d2e97538f2))
* run release-publish.sh with modern bash on hosted runner ([98e0c63](https://github.com/ariso-ai/oats/commit/98e0c63d191ec418039c1dd214b6b06bc72e81cb)), closes [#114](https://github.com/ariso-ai/oats/issues/114)
* show Play button for local recordings in Transcript tab ([7a96d56](https://github.com/ariso-ai/oats/commit/7a96d56f9e22c77063c33e9451aaa233456322d1))

## [0.8.0](https://github.com/ariso-ai/oats/compare/v0.7.2...v0.8.0) (2026-06-18)


### Features

* friendly default title for local meetings ([af1566d](https://github.com/ariso-ai/oats/commit/af1566d61670b5860b140ecce94438ed9606f8a9))
* friendly default title for local meetings ([d15fced](https://github.com/ariso-ai/oats/commit/d15fcedb1ed2d761b8def8c14be0c4df6100c670))
* **local:** add Regenerate notes button on the AI Notes tab ([7b04cc1](https://github.com/ariso-ai/oats/commit/7b04cc1d270251f014ca3161b8622880c4226646))


### Bug Fixes

* **local:** clear stale ari-note.md on notes regeneration so it's observable ([59fb01f](https://github.com/ariso-ai/oats/commit/59fb01ff6bfa65568eb29528bc65b357518a75e2))
* **tray:** keep Meetings menu item visible while recording ([c128c42](https://github.com/ariso-ai/oats/commit/c128c42ff92d8c3fc79aadce2cae6ab3f3920df6))
* **tray:** keep Meetings menu item visible while recording ([8c1c298](https://github.com/ariso-ai/oats/commit/8c1c29853758493fcd16220302ce744aef81f710))

## [0.7.2](https://github.com/ariso-ai/oats/compare/v0.7.1...v0.7.2) (2026-06-17)


### Bug Fixes

* lazy-load router views to isolate import failures ([f8abec7](https://github.com/ariso-ai/oats/commit/f8abec74bb8647971c7d316502313a89496a2be3))
* lazy-load router views to isolate import failures ([1d50e2d](https://github.com/ariso-ai/oats/commit/1d50e2de30ecd21ed758bd215d384d68279deeb0))

## [0.7.1](https://github.com/ariso-ai/oats/compare/v0.7.0...v0.7.1) (2026-06-17)


### Bug Fixes

* improves app icon ([ab83daf](https://github.com/ariso-ai/oats/commit/ab83daf66fa5361ce6bf88a9a59b53d45b95c25e))
* improves app icon ([45bcd5b](https://github.com/ariso-ai/oats/commit/45bcd5b1b5681c5cebddd7a289c2d0c82afe7a55))

## [0.7.0](https://github.com/ariso-ai/oats/compare/v0.6.0...v0.7.0) (2026-06-16)


### Features

* local-meeting search dialog in titlebar ([d6aa308](https://github.com/ariso-ai/oats/commit/d6aa308eb1128af30a22970e9edcdbe51db4d209))


### Bug Fixes

* avoid auto loading a meeting into detail view ([6bf4b1e](https://github.com/ariso-ai/oats/commit/6bf4b1ebf9af244fced492a15254b1679330726c))
* avoid auto loading a meeting into detail view ([1ed1ad6](https://github.com/ariso-ai/oats/commit/1ed1ad679f2f86594357d2eda72039d25547c020))
* capture mic raw so recording doesn't lower the user's voice on calls ([992ce08](https://github.com/ariso-ai/oats/commit/992ce0807fe454276813bd4cd41816c3abc33101))
* keep the same search UX ([a6bc6df](https://github.com/ariso-ai/oats/commit/a6bc6df7e61ded892034ab0ddec420f4ffd8f453))
* remove sidecar header ([b1e4eab](https://github.com/ariso-ai/oats/commit/b1e4eab6ab2b82ccafdf2fb4ecd4730077eb8cfd))
* remove sidecar header ([423085a](https://github.com/ariso-ai/oats/commit/423085aa6a6c50044e98659718bb5447fcb2e7ea))
* show today's upcoming meetings and the next day in Up Next card ([f3456eb](https://github.com/ariso-ai/oats/commit/f3456eb433929276f67e24d086d4ddcfeb2742e1))
* show today's upcoming meetings and the next day in Up Next card ([3e960bb](https://github.com/ariso-ai/oats/commit/3e960bb6fa2f43735c7d9dc8e3e821f2845ae4ff))

## [0.6.0](https://github.com/ariso-ai/oats/compare/v0.5.0...v0.6.0) (2026-06-16)


### Features

* add AI Assessment tab and relocate transcript audio player ([4139d0d](https://github.com/ariso-ai/oats/commit/4139d0d48b7c093c4c3adb4c9e9432a33f9cab64))
* add decideRecordingAction decision function for start-recording button ([1fb96bb](https://github.com/ariso-ai/oats/commit/1fb96bb436ee4c9711e1ee5e0c779f7440c55a9d))
* add Up Next opening screen for the meetings window ([15a605b](https://github.com/ariso-ai/oats/commit/15a605bbe070c0c5ac9d7910d71bc9fe27744a3c))
* AI Assessment tab + transcript audio player ([349dc33](https://github.com/ariso-ai/oats/commit/349dc33c8dafd9b4a03fbae8ce0ad3a04ea1c77a))
* customize DMG installer layout ([8f14a79](https://github.com/ariso-ai/oats/commit/8f14a79613dde87737e24be9e636e9409ac0943f))
* drive start-recording button from the active nav view ([1a140ce](https://github.com/ariso-ai/oats/commit/1a140ceb835ab106fae79a86406c30919b349788))
* nav-aware start-recording button + direct-create meeting picker ([334a120](https://github.com/ariso-ai/oats/commit/334a120e1c5182a57f92b2f3204df129c9f6ef6a))
* picker creates a meeting directly when none exist today ([558c1e8](https://github.com/ariso-ai/oats/commit/558c1e8b5261331e6085f0879b603438443f8ffa))
* Up Next opening screen for the meetings window ([578545f](https://github.com/ariso-ai/oats/commit/578545ff8dc9f70531e2887a9b9878f6141596aa))


### Bug Fixes

* fix article grammar ([44ef647](https://github.com/ariso-ai/oats/commit/44ef647c665ef1e7804d1c1c7af9f0af1abe5f10))
* group Meetings list purely by date, drop UPCOMING section ([a19e867](https://github.com/ariso-ai/oats/commit/a19e8675f6fe3d6c774f983b683b347ef52573eb))
* make the My Notes title editable ([e845144](https://github.com/ariso-ai/oats/commit/e8451446683efa2e88f41fc727cd51bdee07b19f))
* make the My Notes title editable ([523c65e](https://github.com/ariso-ai/oats/commit/523c65e231cb1103e2f26ec60734c91eaab8b84a))

## [0.5.0](https://github.com/ariso-ai/oats/compare/v0.4.0...v0.5.0) (2026-06-15)


### Features

* add Ariso meeting share HTTP methods ([9553cd9](https://github.com/ariso-ai/oats/commit/9553cd99183cd11015de62ae46f1aa72de78d058))
* add local-share markdown composer ([93662da](https://github.com/ariso-ai/oats/commit/93662dad0a8deee0a57c8e23d82fcd2b1d69609f))
* add native macOS share_text command ([641e879](https://github.com/ariso-ai/oats/commit/641e87939426c32ac496906a29f16e51cdf36317))
* add Resume control to the failed recorder pill ([f4f3d1d](https://github.com/ariso-ai/oats/commit/f4f3d1d17cc16caa6257de437071b3ef95e380d0))
* add ShareMeetingPopover component ([83935ec](https://github.com/ariso-ai/oats/commit/83935eca5679c4625719daf3a9686b8bdcb376c3))
* append resumed audio to the failed recording on stop ([28722d3](https://github.com/ariso-ai/oats/commit/28722d3f49438f47cbd9640e97219df1e643e073))
* Ariso finalize buffers the full pending-upload meta ([5454c6f](https://github.com/ariso-ai/oats/commit/5454c6feddd5cfbe0cfc2c42469eddbdb3adc304))
* buffer-with-meta, list, and combine pending-upload commands ([a419207](https://github.com/ariso-ai/oats/commit/a419207b17e7cbcdd4335615680f685452f23a8f))
* combine_pending_audio concatenates buffered mp3s by key ([fcdc238](https://github.com/ariso-ai/oats/commit/fcdc238003b6d2f17acd54d40344113d83a295d2))
* list_pending_uploads scans/pairs/sorts buffered uploads ([26ff892](https://github.com/ariso-ai/oats/commit/26ff892243368d39cae7c4769767f1a68ce55183))
* meeting sharing in the desktop detail panel ([8a07059](https://github.com/ariso-ai/oats/commit/8a07059daf19608f0826c90d364edb46cc59cc6b))
* **meetings:** record a new meeting from the picker ([78be690](https://github.com/ariso-ai/oats/commit/78be6903aa34655dc72e1d7e00a2c2ee808902b2))
* **meetings:** record a new meeting from the picker ([c951c06](https://github.com/ariso-ai/oats/commit/c951c06ecf24202410db01ec6147e89ed42c1a1a))
* **models:** per-target download guards so STT and LLM run in parallel ([8b7792b](https://github.com/ariso-ai/oats/commit/8b7792b9ad83869766e9e57d4b22f85d3d9dd096))
* pending bridge gains meta, list, and combine ([99fbdd6](https://github.com/ariso-ai/oats/commit/99fbdd6a59076cee45a3748f2bda8e9170ead93d))
* PendingUploads sidebar section with Upload/Discard all ([5adaf29](https://github.com/ariso-ai/oats/commit/5adaf291158ee594d2b1e51579693bd8af11c2cc))
* persist a metadata sidecar for pending uploads ([c307296](https://github.com/ariso-ai/oats/commit/c30729634047a3740de7a13cfdfcedd22a2be861))
* recover from failed Ariso audio uploads + in-app audio playback ([7bdce04](https://github.com/ariso-ai/oats/commit/7bdce0426d7681e6f21f3b69e5c7c0648d6063ab))
* resume recording from the failed pill, preserving the held audio ([5d55089](https://github.com/ariso-ai/oats/commit/5d55089d557deea419e1c709479b6347ae405d8f))
* **settings:** first-time confirm dialog downloads both local models in parallel ([d7115e5](https://github.com/ariso-ai/oats/commit/d7115e5769fbced613b7c384ad21a9cafc1d01ea))
* **settings:** first-time local model download confirm + scroll fixes ([80bfd4b](https://github.com/ariso-ai/oats/commit/80bfd4bb3079f18b12b33b4340079916e1f68f15))
* **settings:** persist localModelsPrompted flag ([7cd45d8](https://github.com/ariso-ai/oats/commit/7cd45d8ea845e27da8444974af3a9074fe22c4d6))
* **settings:** shouldPromptDownload gate for first-time local models prompt ([29efa1a](https://github.com/ariso-ai/oats/commit/29efa1ac92e6789cab780ac248c69ee1c50a6c17))
* surface PendingUploads in the Library sidebar ([5b768a3](https://github.com/ariso-ai/oats/commit/5b768a3640c131134d9e0332b954d17da3f6dd5d))
* surface share-gating fields on meeting detail ([c0af9c1](https://github.com/ariso-ai/oats/commit/c0af9c1bdc08d3a88736723ec8e43f485def794e))
* usePendingUploads combine/upload/discard logic ([315b36e](https://github.com/ariso-ai/oats/commit/315b36ed697336baeb4eff63c467cba267950da6))
* wire Share button to Ariso popover and local native share ([599efb6](https://github.com/ariso-ai/oats/commit/599efb636add3d155df0581b22e1a528a5c11c97))


### Bug Fixes

* abandon timed-out finalize on resume so the next upload isn't dropped ([b29955b](https://github.com/ariso-ai/oats/commit/b29955bca3312303601702f2ebf19af4238ee8ab))
* cancel pending discard confirmation on pointer leave ([ef948cd](https://github.com/ariso-ai/oats/commit/ef948cdb2b19e5b75fde7ee6190d00c5b565ebb1))
* match tray icon size across light and dark themes ([06e419c](https://github.com/ariso-ai/oats/commit/06e419c1d1c6966f48022bdc51377d4f75a7bdc9))
* match tray icon size across light and dark themes ([4c478c8](https://github.com/ariso-ai/oats/commit/4c478c831c4a141d7635f887a4fd1ee0643b774e))
* **recorder:** stop the vertical pill flashing over the meetings window ([b1208dc](https://github.com/ariso-ai/oats/commit/b1208dcf00409044769fb1f4d3ffab22073579b0))
* **recorder:** stop the vertical pill flashing over the meetings window ([5b6b09c](https://github.com/ariso-ai/oats/commit/5b6b09cd616b6db155d6462fd0107ef8abb463d0))
* **recording:** require Ariso sign-in at in-app recording entry points ([8042f4a](https://github.com/ariso-ai/oats/commit/8042f4ae5d996060db6ad737dc8e624b3898c1a6))
* **recording:** require Ariso sign-in at in-app recording entry points ([b57c554](https://github.com/ariso-ai/oats/commit/b57c554e78c8f4e57ace43747358e30d87d5e447))
* **settings:** hide the scrollbar chrome while keeping vertical scroll ([f897699](https://github.com/ariso-ai/oats/commit/f897699bb7dc26e833d63c41ea2e33373573cd59))
* **settings:** make the settings window scroll vertically when content overflows ([45f0d9f](https://github.com/ariso-ai/oats/commit/45f0d9f7797381615110b4d296e5fa7fb5a0ab63))
* **settings:** scope system audio to "System Audio Recording Only" + highlight active backend ([f943b34](https://github.com/ariso-ai/oats/commit/f943b34b7bcb7ac6aa10722c3197e61e922aa88e))
* **settings:** scope system audio to "System Audio Recording Only" + highlight active backend ([46bd84f](https://github.com/ariso-ai/oats/commit/46bd84f23bb2485ec0892828562f9125b45b209f))
* show the recorder window before getUserMedia so capture can start ([93a4be7](https://github.com/ariso-ai/oats/commit/93a4be79578fc29816b02621b51f4bd38a15f483))
* stop the recording red dot from lingering after a failed upload ([bbf7791](https://github.com/ariso-ai/oats/commit/bbf779137f708c62b3515685f6bd7720c848d1bc))

## [0.4.0](https://github.com/ariso-ai/oats/compare/v0.3.4...v0.4.0) (2026-06-13)


### Features

* automate releases with release-please ([f2994ea](https://github.com/ariso-ai/oats/commit/f2994eac617426d94d31c66d68844c9313364353))
* automate releases with release-please ([d316481](https://github.com/ariso-ai/oats/commit/d316481c96322daf98d3ade549de52d8c4e1f1fa))
* rename + new logo ([dddcb83](https://github.com/ariso-ai/oats/commit/dddcb837c05bfe0a973a0de547d3a1909fad2c06))
* rename the product from Ariso to oats ([555b4fd](https://github.com/ariso-ai/oats/commit/555b4fd5e6d0adafba4ae57451b13ec642abdcdc))
* rename the product from Ariso to oats ([b85aaf4](https://github.com/ariso-ai/oats/commit/b85aaf491a830ed235a12b4b689d2d6bf4c6a09d))
