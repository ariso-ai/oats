# OATS listing PR batch report

This report summarizes the OpenProse-native listing campaign for OATS. The
purpose of the run was to open draft pull requests in external list repositories,
not to open a review PR against `ariso-ai/oats`.

Run id: `20260707-155004-rg3uum`

Branch-hosted report URL:
`https://github.com/ariso-ai/oats/blob/feat/openprose-listing-campaign/docs/outreach/listing-pr-batch-report.md`

## Summary

| Result | Count |
|---|---:|
| Selected GitHub list targets | 48 |
| Draft external PRs opened | 31 |
| Targets skipped after repo-specific review | 17 |
| OATS repository PRs opened for this campaign | 0 |

The accidental OATS-side PR `ariso-ai/oats#221` was closed and is not part of
the campaign result. This branch exists only to host the public report and
source campaign artifacts that external maintainers can inspect.

## Source Artifacts

- Research report: [`oats-listing-target-research.md`](oats-listing-target-research.md)
- Structured target manifest: [`listing-targets.json`](listing-targets.json)
- Canonical OATS listing profile: [`oats-listing-profile.md`](oats-listing-profile.md)
- OpenProse contract: [`../../.agents/prose/src/oats-listing-campaign/index.prose.md`](../../.agents/prose/src/oats-listing-campaign/index.prose.md)

## Draft PRs

| Target repo | Draft PR | Placement | Validation notes |
|---|---|---|---|
| `jaywcjlove/awesome-mac` | [#2268](https://github.com/jaywcjlove/awesome-mac/pull/2268) | Note-taking | Follow-up placement fix moved OATS out of Voice-to-Text across all four README languages; `git diff --check`, `npm run build`, and `npm run create:ast` passed. Hosted FOSSA check was still pending. |
| `serhii-londar/open-source-mac-os-apps` | [#1188](https://github.com/serhii-londar/open-source-mac-os-apps/pull/1188) | `applications.json` notes app block | `git diff --check` passed. Swift generator passed; full awesome_bot failed on pre-existing unrelated target links. |
| `iCHAIT/awesome-macOS` | [#909](https://github.com/iCHAIT/awesome-macOS/pull/909) | Applications / Productivity | Upstream awesome_bot fails on pre-existing broken links; rerun with unrelated failures whitelisted passed. |
| `phmullins/awesome-macos` | [#210](https://github.com/phmullins/awesome-macos/pull/210) | Notes | No configured tests, lints, builds, or typechecks; `git diff --check` passed. |
| `viraat/awesome-mac-apps` | [#1](https://github.com/viraat/awesome-mac-apps/pull/1) | Studying/Researching | Follow-up placement fix moved OATS from Menu bar apps beside the existing notetaking/audio-notes entry; no configured tests, lints, builds, or typechecks; `git diff --check` passed. |
| `jordanbaird/awesome-menubar` | [#1](https://github.com/jordanbaird/awesome-menubar/pull/1) | Apps | Added first Apps section; no configured tests, lints, builds, or typechecks; `git diff --check` passed. |
| `tborychowski/awesome-mac` | [#5](https://github.com/tborychowski/awesome-mac/pull/5) | Notes and writing / Contenders | No contribution guide or configured checks; `git diff --check` passed. |
| `feep/awesome-apple-silicon` | [#3](https://github.com/feep/awesome-apple-silicon/pull/3) | Native ARM software / Native apps | No configured tests, lints, builds, or typechecks; `git diff --check` passed. |
| `smashism/awesome-macadmin-tools` | [#57](https://github.com/smashism/awesome-macadmin-tools/pull/57) | Productivity | Low-likelihood Mac-admin fit; `git diff --check` passed. |
| `pluja/awesome-privacy` | [#912](https://github.com/pluja/awesome-privacy/pull/912) | Notes and Tasks | Follow-up placement fix moved OATS from Speech to Text apps to Notes and Tasks. Local format and whitespace checks passed; follow-up `git diff --check` passed and the OATS link resolved. Full README lychee fails on baseline links. |
| `iAnonymous3000/awesome-privacy-tools` | [#33](https://github.com/iAnonymous3000/awesome-privacy-tools/pull/33) | Notes | README-only repo with no configured checks; `git diff --check` passed. |
| `janhq/awesome-local-ai` | [#132](https://github.com/janhq/awesome-local-ai/pull/132) | User Tools | No configured tests, lints, builds, or typechecks; `git diff --check` passed. |
| `msb-msb/awesome-local-ai` | [#19](https://github.com/msb-msb/awesome-local-ai/pull/19) | User Interfaces / Desktop Applications | Used a renamed fork because the default fork name belonged to another fork network; `git diff --check` passed. |
| `rafska/awesome-local-llm` | [#132](https://github.com/rafska/awesome-local-llm/pull/132) | Tools / Miscellaneous | Medium fit; no configured checks; `git diff --check` passed. |
| `alexanderop/awesome-local-first` | [#44](https://github.com/alexanderop/awesome-local-first/pull/44) | Example Applications / Productivity and Collaboration | No configured checks; `git diff --check` passed. |
| `schickling/awesome-local-first` | [#40](https://github.com/schickling/awesome-local-first/pull/40) | Applications / Projects | Used a correctly parented fork because an existing fork pointed elsewhere; `git diff --check` passed. |
| `alantriesagain/awesome-local-first` | [#10](https://github.com/alantriesagain/awesome-local-first/pull/10) | Applications / Productivity and Knowledge Management | No configured checks; `git diff --check` passed. |
| `zhongkechen/awesome-local-first` | [#7](https://github.com/zhongkechen/awesome-local-first/pull/7) | Applications / Meeting Notes | No configured checks; `git diff --check` passed. |
| `wq2012/awesome-diarization` | [#46](https://github.com/wq2012/awesome-diarization/pull/46) | Products | No configured checks; `git diff --check` passed. |
| `steven2358/awesome-generative-ai` | [#1023](https://github.com/steven2358/awesome-generative-ai/pull/1023) | `DISCOVERIES.md` / Text / Meeting assistants | New OATS links returned HTTP 200; `git diff --check` passed. awesome_bot/awesome-lint failures were pre-existing baseline issues. |
| `alvinreal/awesome-opensource-ai` | [#575](https://github.com/alvinreal/awesome-opensource-ai/pull/575) | Desktop and Mobile AI Apps | `python3 tools/validate_awesome.py --skip-remote` and `git diff --check` passed. |
| `suncloudsmoon/awesome-open-source-ai` | [#15](https://github.com/suncloudsmoon/awesome-open-source-ai/pull/15) | Tools | No configured checks; `git diff --check` passed; GitHub reported no PR checks. |
| `tehtbl/awesome-note-taking` | [#108](https://github.com/tehtbl/awesome-note-taking/pull/108) | Open Source / Tauri | `git diff --check` and markdownlint passed; one pre-existing trailing space was fixed. |
| `nil0x42/awesome-hacker-note-taking` | [#8](https://github.com/nil0x42/awesome-hacker-note-taking/pull/8) | App list near SwiftnessX and JupyterPen | No configured checks; `git diff --check` passed. |
| `knowfox/awesome-pkm` | [#4](https://github.com/knowfox/awesome-pkm/pull/4) | Tools | No configured checks; `git diff --check` passed. |
| `doanhthong/awesome-pkm` | [#16](https://github.com/doanhthong/awesome-pkm/pull/16) | Note-taking Tools / Open-source | No configured checks; `git diff --check` passed; upstream appears stale. |
| `brettkromkamp/awesome-knowledge-management` | [#56](https://github.com/brettkromkamp/awesome-knowledge-management/pull/56) | Platforms, Applications and Tools | No configured checks; `git diff --check` passed. |
| `jyguyomarch/awesome-productivity` | [#336](https://github.com/jyguyomarch/awesome-productivity/pull/336) | Tools and Apps / Note Management | `git diff --check` and `npx --yes awesome-lint` passed after minimal existing README wording fixes. |
| `ProductivityDirectory/awesome-productivity-tools` | [#88](https://github.com/ProductivityDirectory/awesome-productivity-tools/pull/88) | Note Taking | No configured checks; `git diff --check` passed; GitHub reported no PR checks. |
| `mundimark/awesome-markdown-editors` | [#190](https://github.com/mundimark/awesome-markdown-editors/pull/190) | `UPCOMING.md` / Markdown Desktop Editors / Apple Mac OS X | OATS is a Markdown-generating meeting-notes app, not a general Markdown editor; target scope accepts upcoming Markdown note-taking/workspace apps. |
| `BubuAnabelas/awesome-markdown` | [#131](https://github.com/BubuAnabelas/awesome-markdown/pull/131) | Tools / Miscellaneous | Yarn install exited 0 with optional `fsevents` build failure; remark exited 0 with pre-existing warning-level README issues; `git diff --check` passed. |

## Skipped Targets

| Target repo | Candidate placement | Reason |
|---|---|---|
| `antelle/my-awesome-mac-apps` | N/A | README says contributions are not welcome; list is personal and apps must be ones the maintainer uses and loves. |
| `lissy93/awesome-privacy` | Productivity / Digital Notes | Target rules require a non-new repo and first stable release older than four months; OATS is too recent. |
| `awesome-selfhosted/awesome-selfhosted` | N/A | Direct PRs are refused and the list is for self-hosted network services/web apps; OATS is a native macOS app. |
| `vince-lam/awesome-local-llms` | Applications | README is generated from Turso data, suggestions are issue-based, and OATS is below the 100-star threshold. |
| `haiiiiiyun/awesome-selfhosted-cn` | N/A | Scope is locally hosted network services/web apps requiring server-side platform metadata; OATS is a desktop app. |
| `sindresorhus/awesome-whisper` | N/A | Target rejects draft/WIP PRs, requires at least 100 stars, and is Whisper-specific; OATS is not Whisper-based. |
| `danielrosehill/Awesome-Whisper-Apps` | N/A | Meeting-notes section exists, but repo scope is OpenAI Whisper apps; OATS uses FluidAudio/Parakeet. |
| `ancs21/awesome-openai-whisper` | N/A | Target scope is explicitly OpenAI Whisper; OATS is not documented as Whisper-based. |
| `MIBlue119/awesome-whisper-application` | N/A | Target README scopes entries to Whisper-based apps; OATS is not documented as Whisper-based. |
| `primaprashant/awesome-voice-typing` | N/A | Target requires active dictation/voice typing workflows and excludes general transcription or meeting-note tools without that interface. |
| `zzw922cn/awesome-speech-recognition-speech-synthesis-papers` | N/A | Bibliography-only paper list; no accepted tools/apps/software section. |
| `goldsmith/awesome-speech-recognition-papers` | N/A | Bibliography-only ASR paper roadmap; no apps/tools/software section. |
| `swiftsimplify/awesome-open-source-ai-tools` | Music and Audio or Other AI Tools | Requires at least 50 GitHub stars or demonstrated community adoption; OATS is below that bar. |
| `spsdco/notes` | N/A | Springseed application source, not a curated listing/publication repo. |
| `githubkusi/awesome-knowledge-management-tools` | N/A | Scope is collaborative/corporate knowledge management and excludes non-collaborative PKM-style tools. |
| `areknawo/awesome-productivity-software` | Notes | Repo is archived/read-only, so it is not accepting PRs. |
| `mundimark/awesome-markdown` | N/A | Scope is Markdown building blocks such as libraries and engines, not apps that only export Markdown. |

## Deferred Surfaces

The broader manifest still includes GitHub topics, form submissions, editorial
directories, and package/index PR targets. Those were intentionally not treated
as this external list-PR batch. They need separate manual or package-specific
follow-up.

## Review Guidance

All opened PRs were created as drafts where GitHub allowed draft mode. Review
should focus on whether each entry accurately fits that list's scope and whether
any low-likelihood placements should be closed before maintainers review them.

A post-creation placement audit corrected three stronger-fit category misses:
`jaywcjlove/awesome-mac#2268` now uses Note-taking instead of Voice-to-Text,
`pluja/awesome-privacy#912` now uses Notes and Tasks instead of Speech to Text,
and `viraat/awesome-mac-apps#1` now uses Studying/Researching instead of Menu
bar apps. The remaining generic placements did not have an available
notes/meeting-notes section in their target repo structure.

The lowest-confidence live PRs are:

- [`smashism/awesome-macadmin-tools#57`](https://github.com/smashism/awesome-macadmin-tools/pull/57): OATS is productivity tooling for Mac users/admins, not fleet-management software.
- [`rafska/awesome-local-llm#132`](https://github.com/rafska/awesome-local-llm/pull/132): acceptable app-layer local AI fit, but broader than core LLM infrastructure.
- [`mundimark/awesome-markdown-editors#190`](https://github.com/mundimark/awesome-markdown-editors/pull/190): OATS produces Markdown notes but is not a general Markdown editor.
