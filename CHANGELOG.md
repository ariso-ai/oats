# Changelog

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
