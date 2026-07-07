# Progress

## 2026-07-07

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
