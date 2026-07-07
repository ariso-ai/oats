# Progress

## 2026-07-07

### Target publication worker: phmullins/awesome-macos

- Approach: run a scoped OpenProse listing worker for the OATS campaign, keeping target-repo scratch work under `.outreach/listing-prs/phmullins-awesome-macos`.
- Steps taken so far: loaded the OpenProse skill; confirmed GitHub auth for `mchlggr`; inspected target repo metadata, open PRs, README structure, license, and contribution surfaces; cloned `phmullins/awesome-macos` into the requested scratch directory; checked for duplicate OATS-related PRs.
- Findings so far: the target has no formal contribution file and uses a single `readme.md` with simple bullets plus icon tags. OATS honestly fits the `Notes` section because the primary user value is meeting notes, transcripts, speaker labels, and summaries. `Menubar Applications` is less precise because that section is mostly menu-bar utilities.
- Outcome: added one `oats` entry under `### Notes`, committed `docs: add oats`, pushed branch `docs/add-oats-notes-20260707` to `mchlggr/awesome-macos`, and opened draft PR https://github.com/phmullins/awesome-macos/pull/210.
- Validation: `git diff --check` passed before and after commit. The target repo has no configured test, lint, build, or typecheck command files, so there were no target test suites to run for this README-only change. Final PR metadata confirmed draft=true, base=`master`, head=`docs/add-oats-notes-20260707`, and changed file=`readme.md`.
- Failures/lessons: literal `/progress.md` is on a read-only filesystem root, so progress is being written to this existing repo-level `progress.md` instead. No callable thread-title tool was exposed after tool discovery, so the conversation-title prefix instruction could not be applied programmatically from this session.

- Approach: run a single OpenProse target-publication worker for
  `viraat/awesome-mac-apps`, keeping the target clone and all target-repo work
  under `.outreach/listing-prs/viraat-awesome-mac-apps`.
- Steps taken so far: loaded the OpenProse skill; confirmed GitHub auth as
  `mchlggr`; checked the campaign target manifest and public campaign report;
  inspected `viraat/awesome-mac-apps` metadata, default branch, open PRs, open
  issues, recent PR history, and local README structure; cloned the target repo
  into the requested scratch path.
- Findings so far: the target repo has only `README.md`, no separate
  contribution guide, no open PRs/issues, and the README explicitly welcomes
  issues/PRs. It describes the list as open-source Mac apps and says apps are
  free unless marked paid, so OATS honestly fits as a free/open-source
  local-first meeting-notes app.
- Current plan: add one OATS entry under `## Menu bar apps`, matching the
  existing `[Name](url): description` style; commit with `docs: add oats`; push
  a unique branch to the authenticated user's fork; open a draft PR linking the
  public campaign report.
- Failures/lessons so far: no callable thread-title tool was exposed after
  tool discovery, so the conversation-title prefix instruction could not be
  applied programmatically from this session.
- Completed target-publication result: created fork
  `mchlggr/awesome-mac-apps`, pushed branch
  `feat/add-oats-viraat-20260707155328`, committed `docs: add oats`, and opened
  draft PR https://github.com/viraat/awesome-mac-apps/pull/1 against
  `viraat/awesome-mac-apps:master`.
- Placement/edit: added
  `[OATS](https://github.com/ariso-ai/oats): Open-source local-first meeting-notes app for Apple Silicon Macs with live transcription, speaker labels, AI summaries, and an offline on-device mode.`
  under `README.md` > `## Menu bar apps`, after `itsycal` and before
  `Tomato One`.
- Validation: `git diff --check` passed before commit and
  `git diff --check HEAD~1..HEAD` passed after commit. The target repo only
  contains `README.md` and has no configured tests, lint, build, or typecheck
  commands to run.
- Failures/lessons: `gh repo fork viraat/awesome-mac-apps --clone=false
  --remote=false` failed because this `gh` version does not support `--remote`
  when a repository argument is supplied; using `gh api -X POST
  repos/viraat/awesome-mac-apps/forks -F default_branch_only=true` and adding
  the `fork` remote explicitly worked. The first README edit introduced trailing
  whitespace and dropped the file's final blank line; `git diff --check` caught
  the whitespace, and the final diff was reduced to one insertion.

- Approach: revise the outreach work to be OpenProse-native instead of a
  JavaScript batch runner. The workflow is now a Contract Markdown function that
  reads the source report and target manifest, fans out selection and publisher
  workers, opens draft PRs only in explicit live mode, and writes a review report
  linking the draft PRs.
- Context gathered: loaded the requested `/Users/michael/.agents/skills/open-prose`
  skill, Contract Markdown spec, OpenProse tenets, authoring guidance, and
  ProseScript syntax. Corrected the earlier assumption: `prose run` is embodied
  by a Prose Complete host such as Codex or Claude Code; this repo should not
  shell out to a standalone `prose` binary for this campaign.
- Steps taken: converted the attached OATS listing research report into
  `docs/outreach/listing-targets.json` with 103 targets, including 66 automated
  GitHub list PR targets, 22 GitHub topic updates, 7 form submissions, 5 manual
  editorial targets, and 3 package PR targets; added the source report at
  `docs/outreach/oats-listing-target-research.md`; added
  `docs/outreach/listing-pr-batch-report.md` as the PR-linked public campaign
  report; added `docs/outreach/oats-listing-profile.md` for canonical copy; added
  `.agents/prose/src/oats-listing-campaign/index.prose.md`; removed the rejected
  Node runner and legacy `.prose` script; ignored OpenProse generated deps, dist,
  runs, and env files.
- Failures/notes: `npm ls --depth=0` initially showed all npm dependencies
  missing because `node_modules/` was not installed in this worktree.
  `npm ci --prefer-offline` succeeded, but npm reported two existing
  high-severity audit findings. The first outreach design was too code-driven
  and treated OpenProse like an external CLI; the corrected design keeps the
  orchestration in `.prose.md` and lets the host supply worker sessions.
- Validation: JSON structure checks passed for `package.json` and
  `docs/outreach/listing-targets.json`; the manifest count check passed with 103
  targets and the expected submission-kind split; a lightweight OpenProse
  contract marker check passed; `gh auth status` confirmed the host is ready for
  live GitHub work; `npx vitest run src/views/WaveformView.test.ts` passed; full
  `npm test` passed with 48 files and 507 tests; `npm run vite:build` passed with
  the existing large-chunk warning. The first full `npm test` run failed on stale
  `UpdateView` and `LibraryView` expectations, so those tests were updated:
  update tests now use `__APP_VERSION__`, the LibraryView detail fixture includes
  `audioClips`, the detail stub exposes `saveNotesNow`, and recording-start
  assertions now check sidebar collapse rather than disappearance of the titlebar
  Start button. A later full suite run exposed a `WaveformView` fake-timer leak;
  the timer tests now unmount the component before restoring real timers.
- Type/lint notes: there is no `lint` script in `package.json`. There is also no
  configured typecheck script and TypeScript is not installed as a direct project
  tool. A diagnostic `npm exec --package typescript -- tsc --noEmit` run was
  attempted and failed on broad pre-existing app/test typing issues, not on the
  outreach script; the affected script syntax check above passed.
- Lessons learned: the target set should stay structured in JSON, with non-PR
  surfaces kept explicit as manual/form/topic follow-ups rather than forcing
  every listing surface through GitHub PR automation. OpenProse is the workflow
  contract and VM behavior, not a subprocess hidden behind a project script.
  Vue fake-timer tests should clear component intervals before switching timer
  implementations.

### Target publication worker: iCHAIT/awesome-macOS

- Approach: inspect `iCHAIT/awesome-macOS` contribution guidance, README style,
  ordering, relevant sections, and open PRs from a scratch clone under
  `.outreach/listing-prs/ichait-awesome-macos`; only open a draft PR if OATS
  honestly fits the Mac software discovery audience.
- Steps taken: started the target-publication worker flow for target
  `ichait-awesome-macos`; read the local OpenProse skill instructions because
  this task explicitly names OpenProse; confirmed literal `/progress.md` could
  not be written by the patch tool, so this existing repo progress log is the
  active writeback surface; cloned `iCHAIT/awesome-macOS` into the requested
  scratch directory; inspected `.github/contributing.md`, the PR template,
  README sections, open PRs, and OATS metadata; created branch
  `feat/add-oats-20260707`; added a single `README.md` entry for OATS in the
  `Productivity` section between `MenubarX` and `OmniFocus`; validated the
  patch with `git diff --check`, direct HEAD requests for the new product and
  source URLs, and the target repo's `awesome_bot` README check; created a fork
  at `mchlggr/ichait-awesome-macOS`, committed `docs: add oats`, pushed branch
  `feat/add-oats-20260707`, and opened draft PR
  `https://github.com/iCHAIT/awesome-macOS/pull/909`.
- Failures: attempting to create literal `/progress.md` failed due filesystem
  write constraints on that absolute path; the first README patch was pointed
  at the OATS root by default and failed cleanly without modifying the file, so
  it was rerun against the scratch clone path; the unmodified target README's
  exact Travis `awesome_bot` command failed on nine pre-existing broken links,
  so the verification was rerun with only those known unrelated failures
  whitelisted and `--skip-save-results`, which passed.
- Lessons learned: keep target-repo scratch changes isolated under `.outreach`
  and treat the OATS root progress log as the only required exception to the
  "do not modify OATS" constraint; this target's AI-adjacent guidance requires
  positioning OATS as a meeting-notes workflow with on-device processing, not as
  a general AI prompt wrapper; the authenticated user's existing
  `mchlggr/awesome-macos` repository is a fork of `phmullins/awesome-macos`, so
  a distinct fork name was required for the requested `iCHAIT/awesome-macOS`
  target.

### Target publication worker: jordanbaird/awesome-menubar

- Approach: inspect `jordanbaird/awesome-menubar` from a scratch clone under
  `.outreach/listing-prs/jordanbaird-awesome-menubar`; only open a draft PR if
  OATS honestly fits the menu bar app discovery audience.
- Steps taken: read the local OpenProse skill instructions because this task
  explicitly names OpenProse; confirmed GitHub auth as `mchlggr`; cloned the
  target repo into the requested scratch directory; inspected target metadata,
  README, license, open PRs, and open issues; confirmed the repo has only
  `README.md` and `LICENSE`, no contribution guide, no existing list entries,
  and no open PRs or issues.
- Outcome: created fork `mchlggr/awesome-menubar`, created branch
  `feat/add-oats-20260707-1154`, added a new `## Apps` section with one OATS
  entry, committed `docs: add oats`, pushed to the fork, and opened draft PR
  https://github.com/jordanbaird/awesome-menubar/pull/1.
- Placement/edit: added
  `[oats](https://github.com/ariso-ai/oats) - Open-source local-first macOS meeting-notes app with live transcription, speaker labels, AI summaries, and optional fully offline on-device mode.`
  under `README.md` > `## Apps`.
- Validation: `git diff --check` passed. No `markdownlint` executable was
  available, and the target repo has no package/config files defining tests,
  lint, build, or typecheck commands for this README-only change. Final PR
  metadata confirmed draft=true, base=`main`, head=`feat/add-oats-20260707-1154`,
  and changed file=`README.md`.
- Failures/lessons: literal `/progress.md` cannot be created because the
  filesystem root is read-only, so this existing repo progress log remains the
  active writeback surface. No callable thread-title tool was exposed after tool
  discovery, so the conversation-title prefix instruction could not be applied
  programmatically from this session. `gh repo fork` with an explicit repository
  argument failed when combined with `--remote`; retrying from inside the clone
  with `--remote --remote-name fork --default-branch-only` worked.

### Target publication worker: serhii-londar/open-source-mac-os-apps

- Approach: inspect `serhii-londar/open-source-mac-os-apps` from a scratch
  clone under `.outreach/listing-prs/serhii-londar-open-source-mac-os-apps`;
  follow its contribution guidance by editing `applications.json` only; open a
  draft PR if OATS honestly fits the open-source macOS app list.
- Steps taken: read the OpenProse skill instructions because this task names
  OpenProse; confirmed GitHub auth as `mchlggr`; inspected target repo metadata,
  README, `CONTRIBUTING.md`, PR template, workflows, open PRs, and OATS metadata;
  confirmed no existing OATS entry or open OATS PR; created fork
  `mchlggr/open-source-mac-os-apps`; created branch
  `add-oats-open-source-mac-apps-20260707120341`; added one OATS object to
  `applications.json` near the notes apps with `notes`, `productivity`, and
  `menubar` categories; committed `docs: add oats`; pushed the branch; opened
  draft PR https://github.com/serhii-londar/open-source-mac-os-apps/pull/1188.
- Placement/edit: entry title `OATS`; description `Open-source local-first
  macOS meeting-notes app with live transcription, speaker labels, AI summaries,
  and an offline on-device mode.`; repo URL `https://github.com/ariso-ai/oats`;
  official site `https://ariso.ai/oats`; icon and screenshot use raw URLs from
  the OATS repository.
- Validation: `git diff --check` passed; OATS repo/icon/screenshot/site URLs
  all returned HTTP 200; the Swift README generator succeeded once and produced
  the expected generated README/API side effects, which were removed because the
  target contribution guide says to edit `applications.json` instead of
  `README.md`; final PR metadata confirmed draft=true, base=`master`, changed
  file=`applications.json`, and 19 additions.
- Failures/lessons: literal `/progress.md` still cannot be created at the
  filesystem root, so this existing repo progress log remains the active
  writeback surface. The full target PR `awesome_bot applications.json` workflow
  command completed but failed on many pre-existing repository links and
  duplicates; none of the OATS links appeared in the failure list. `gh repo fork
  serhii-londar/open-source-mac-os-apps --remote` is unsupported by this `gh`
  version when a repository argument is provided, but retrying from inside the
  scratch clone with `gh repo fork --remote --remote-name fork
  --default-branch-only` worked.

### OpenProse OATS listing campaign live run

- Approach: continue the OpenProse-native campaign as external target workers,
  not an OATS repository PR. The OATS-side branch remains only as the stable
  public report URL that external PR bodies can link.
- Steps taken: completed batch 3 after recovering from context compaction and
  checking GitHub for duplicate OATS PRs before resuming the workers. Added
  draft PRs for `iAnonymous3000/awesome-privacy-tools`,
  `janhq/awesome-local-ai`, `msb-msb/awesome-local-ai`, and
  `rafska/awesome-local-llm`.
- Skips: skipped `awesome-selfhosted/awesome-selfhosted` because the repo
  refuses direct list PRs and OATS is a native macOS app rather than a
  self-hosted service; skipped `vince-lam/awesome-local-llms` because the README
  is generated from Turso data, suggestions are issue-based, and OATS is below
  that list's 100-star threshold.
- Validation: each posted target worker checked for duplicate OATS PRs and ran
  the target's available validation. The batch-3 target repos had no configured
  test/lint/build/typecheck suites, so workers used `git diff --check`.
- Lessons learned: keep workers from writing this progress file directly during
  parallel publication; the main OpenProse host should merge receipts and
  progress checkpoints after each batch to avoid inconsistent run state.

### OpenProse OATS listing campaign batch 4

- Steps taken: completed the self-hosted/local-first/Whisper batch. Created
  draft PRs for `alexanderop/awesome-local-first`,
  `schickling/awesome-local-first`, `alantriesagain/awesome-local-first`, and
  `zhongkechen/awesome-local-first`.
- Skips: skipped `haiiiiiyun/awesome-selfhosted-cn` because its scope is
  self-hosted network services/web apps, and skipped
  `sindresorhus/awesome-whisper` because the repo rejects draft/WIP PRs, has a
  100-star minimum for GitHub OSS entries, and OATS is not Whisper-based.
- Validation: all four posted local-first PRs were README-only changes with no
  target repo test/lint/build/typecheck config; each worker ran `git diff
  --check`.

### OpenProse OATS listing campaign batch 5

- Steps taken: completed the Whisper/speech-recognition slice. No PRs were
  created because all six targets had scope/rule mismatches.
- Skips: `danielrosehill/Awesome-Whisper-Apps`, `ancs21/awesome-openai-whisper`,
  and `MIBlue119/awesome-whisper-application` are Whisper-specific while OATS
  is documented around FluidAudio/Parakeet rather than Whisper;
  `primaprashant/awesome-voice-typing` explicitly excludes general transcription
  and meeting-note tools without an active dictation interface; `zzw922cn` and
  `goldsmith` speech-recognition repos are bibliography-only paper lists.
- Validation: skip workers still checked duplicate OATS/ariso PRs or issues and
  ran available clean-state checks; no target listings were edited.

### OpenProse OATS listing campaign batch 6

- Steps taken: completed the diarization, open-source AI, generative AI, and
  note-taking slice. Created draft PRs for `wq2012/awesome-diarization`,
  `steven2358/awesome-generative-ai`, `alvinreal/awesome-opensource-ai`,
  `suncloudsmoon/awesome-open-source-ai`, and `tehtbl/awesome-note-taking`.
- Skips: skipped `swiftsimplify/awesome-open-source-ai-tools` because its README
  requires at least 50 GitHub stars or demonstrated community adoption, and OATS
  is below that bar today.
- Validation: workers ran target-local checks where available: `alvinreal`
  passed `python3 tools/validate_awesome.py --skip-remote`, `tehtbl` passed
  markdownlint, and the other README/DISCOVERIES edits passed `git diff
  --check`. The `steven2358` target has pre-existing unrelated
  awesome_bot/awesome-lint baseline failures, but the new OATS links returned
  HTTP 200.

### OpenProse OATS listing campaign batch 7

- Steps taken: completed the note-taking, PKM, and knowledge-management slice.
  Created draft PRs for `nil0x42/awesome-hacker-note-taking`,
  `knowfox/awesome-pkm`, `doanhthong/awesome-pkm`, and
  `brettkromkamp/awesome-knowledge-management`.
- Skips: skipped `spsdco/notes` because it is application source rather than a
  listing repo, and skipped `githubkusi/awesome-knowledge-management-tools`
  because its scope is collaborative/corporate knowledge management and it
  excludes non-collaborative PKM-style tools.
- Validation: posted PRs were README-only list edits and all target workers ran
  `git diff --check`; the target repos did not expose configured test, lint,
  build, or typecheck suites.

### OpenProse OATS listing campaign batch 8

- Steps taken: completed the productivity and Markdown slice. Created draft PRs
  for `jyguyomarch/awesome-productivity`,
  `ProductivityDirectory/awesome-productivity-tools`,
  `mundimark/awesome-markdown-editors`, and
  `BubuAnabelas/awesome-markdown`.
- Skips: skipped `areknawo/awesome-productivity-software` because the repo is
  archived/read-only, and skipped `mundimark/awesome-markdown` because its scope
  is Markdown building blocks such as libraries and engines rather than
  apps/tools that only export Markdown.
- Validation: `jyguyomarch` passed `awesome-lint` after minimal existing wording
  fixes, `BubuAnabelas` passed yarn/remark/diff checks with only pre-existing
  warning-level README issues, and the remaining target edits passed `git diff
  --check`.

### OpenProse OATS listing campaign completion

- Outcome: processed all 48 selected `exact-fit-pr-sprint` GitHub list targets.
  The campaign opened 31 draft PRs in external repositories and skipped 17
  targets after repo-specific rule/scope checks. No OATS repository PR is
  required; the accidental OATS-side PR was closed, and this branch is used only
  to keep the source campaign artifacts and review report in one place.
- Report: replaced `docs/outreach/listing-pr-batch-report.md` with the final
  review table linking every draft PR and every skip reason.
- Final validation: JSON receipt counts passed (`31 + 17 = 48`), all 31
  external PR URLs were confirmed open and draft, `git diff --check` passed,
  `npm test` passed with 48 files and 507 tests, and `npm run vite:build`
  passed with the existing large-chunk warning.
- Lessons learned: Whisper-specific and self-hosted lists should be filtered
  aggressively even when the broad research report mentions transcription or
  local/offline behavior. Star/adoption thresholds should be checked live before
  posting, because several otherwise plausible lists correctly rejected young
  projects.

### External PR body cleanup

- Approach: remove internal campaign/report framing from all external draft PR
  descriptions while preserving each target repository's existing PR template,
  checklist, and short fit explanation.
- Steps taken: edited all 31 external draft PR bodies with `gh pr edit` to
  remove `Campaign report`, `Campaign context`, the branch report URL, and the
  explicit outreach-campaign disclosure line.
- Validation: searched all 31 PR bodies for `listing-pr-batch-report`,
  `Campaign report`, `Campaign context`, `outreach campaign`, and `submitted as
  part`; no matches remained. Reconfirmed all 31 PRs are still open drafts.
- Lessons learned: external list PR bodies should follow the destination repo's
  PR style and should not expose internal orchestration or campaign language.

### External PR placement audit

- Approach: audit the open external draft PRs for cases where OATS landed in a
  voice/transcription/menu/generic bucket even though the target repo had a
  stronger notes, meeting-notes, tasks, or studying category.
- Steps taken: updated and pushed `jaywcjlove/awesome-mac#2268` so OATS appears
  in Note-taking across the English, Chinese, Japanese, and Korean READMEs;
  updated `pluja/awesome-privacy#912` so OATS appears in Notes and Tasks; and
  updated `viraat/awesome-mac-apps#1` so OATS appears in Studying/Researching
  beside the existing OneNote entry. Edited the three draft PR titles/bodies to
  match the corrected placements and kept them free of campaign/report wording.
- Validation: all three target repos passed `git diff --check`;
  `jaywcjlove/awesome-mac` also passed `npm run build` and
  `npm run create:ast`; the OATS GitHub link resolved with `curl -I -L --fail`;
  and all three PRs were reconfirmed as draft PRs after the updates.
- Lessons learned: when OATS is submitted to broad app lists, the primary
  category should be notes, meeting notes, recorder, or studying before generic
  voice-to-text, transcription, or menu-bar categories if the list offers both.

### External PR wording cleanup

- Approach: replace product-description language that framed OATS as a
  `menu bar meeting-notes app` with `local-first meeting-notes app`, while
  preserving target-specific categories such as `awesome-menubar` and
  `menubar` metadata where they are part of the destination list structure.
- Steps taken: updated and pushed wording-only follow-up commits to the affected
  external PR branches for `jaywcjlove/awesome-mac`,
  `serhii-londar/open-source-mac-os-apps`, `iCHAIT/awesome-macOS`,
  `phmullins/awesome-macos`, `jordanbaird/awesome-menubar`,
  `feep/awesome-apple-silicon`, `schickling/awesome-local-first`,
  `zhongkechen/awesome-local-first`, `alexanderop/awesome-local-first`,
  `smashism/awesome-macadmin-tools`, `alvinreal/awesome-opensource-ai`,
  `mundimark/awesome-markdown-editors`, `tehtbl/awesome-note-taking`,
  `suncloudsmoon/awesome-open-source-ai`, and
  `nil0x42/awesome-hacker-note-taking`.
- PR bodies: edited the live descriptions for the open PRs that still used the
  old wording, including `open-source-mac-os-apps#1188`,
  `awesome-macOS#909`, `awesome-macos#210`, `awesome-menubar#1`,
  `tborychowski/awesome-mac#5`, `awesome-macadmin-tools#57`,
  `janhq/awesome-local-ai#132`, `alexanderop/awesome-local-first#44`,
  `schickling/awesome-local-first#40`, `awesome-opensource-ai#575`,
  `awesome-note-taking#108`, and `awesome-hacker-note-taking#8`.
- Validation: all touched target repos passed `git diff --check`; additional
  target checks passed for `jaywcjlove/awesome-mac` (`npm run build` and
  `npm run create:ast`), `serhii-londar/open-source-mac-os-apps`
  (`swift .github/main.swift`, with generated README/api side effects removed),
  `alvinreal/awesome-opensource-ai`
  (`python3 tools/validate_awesome.py --skip-remote`), and
  `tehtbl/awesome-note-taking` (markdownlint).
