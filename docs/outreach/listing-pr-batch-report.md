# oats listing PR batch report

This report supports the draft PR batch for adding oats to public software lists
and catalogs. External PR bodies should link here so maintainers can inspect the
campaign scope, source manifest, and positioning in one place.

## Scope

- Source research identified 103 deduplicated listing targets.
- The OpenProse campaign posts only GitHub PR-capable list targets.
- GitHub topic updates, form submissions, editorial directories, and package
  indexes that need deeper packaging work remain visible in the manifest but are
  not silently treated as list PRs.

## Target mix

| Submission kind | Count | Handling |
|---|---:|---|
| GitHub list PR | 66 | OpenProse worker PRs, draft by default |
| GitHub topic | 22 | Manual repo metadata update |
| Form submission | 7 | Manual directory submission |
| Manual/editorial | 5 | Manual review before outreach |
| Package PR | 3 | Manual packaging pass |

The source research report is
[`docs/outreach/oats-listing-target-research.md`](oats-listing-target-research.md).
The structured manifest derived from it is
[`docs/outreach/listing-targets.json`](listing-targets.json).

## OpenProse path

The campaign lives at
[`../../.agents/prose/src/oats-listing-campaign/index.prose.md`](../../.agents/prose/src/oats-listing-campaign/index.prose.md).
It is native OpenProse Contract Markdown, so it should be run by a Prose Complete
host that embodies `prose run`; it should not shell out to a `prose` binary.

The contract does this:

1. Read the source report and structured manifest.
2. Fan out target-selection workers to keep GitHub list PR targets that plausibly
   fit OATS, using a liberal post-split decision.
3. Defer GitHub topics, forms, editorial directories, and package indexes into
   follow-up lanes.
4. In live mode, fan out one publisher worker per selected target. Each worker
   inspects the target repository, finds the right section, makes one minimal
   listing edit, commits with a Conventional Commit message, and opens a draft
   PR when possible.
5. Write a local operator report under `.outreach/listing-pr-runs/` with links
   to the draft PRs, skipped targets, and deferred follow-ups.

## Preferred prompt

Use a dry run first:

```text
prose run .agents/prose/src/oats-listing-campaign/index.prose.md

Parameters:
- research_report_path: docs/outreach/oats-listing-target-research.md
- target_manifest_path: docs/outreach/listing-targets.json
- oats_profile_path: docs/outreach/oats-listing-profile.md
- phase: exact-fit-pr-sprint
- limit: 10
- dry_run: true
- draft_mode: true
```

After reviewing the dry-run report, run a small live batch:

```text
prose run .agents/prose/src/oats-listing-campaign/index.prose.md

Parameters:
- research_report_path: docs/outreach/oats-listing-target-research.md
- target_manifest_path: docs/outreach/listing-targets.json
- oats_profile_path: docs/outreach/oats-listing-profile.md
- phase: exact-fit-pr-sprint
- limit: 3
- dry_run: false
- draft_mode: true
```

## PR positioning

oats should be proposed as an open-source macOS menu bar meeting-notes app with
live transcription, speaker labels, AI summaries, and a fully offline on-device
mode. The canonical listing profile lives in
[`docs/outreach/oats-listing-profile.md`](oats-listing-profile.md).

External PR descriptions should be short and human:

```text
Adds OATS to the [section] list.

OATS fits here as an open-source macOS menu bar app for meeting transcription
and AI notes, with an optional fully on-device mode for privacy-sensitive work.
I kept the edit to one listing entry and matched the existing format.

Campaign context: <public campaign report URL>
```
