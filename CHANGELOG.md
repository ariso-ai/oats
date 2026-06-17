# Changelog

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
