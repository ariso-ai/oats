---
name: oats-listing-campaign
kind: function
---

# OATS Listing Campaign

### Description

OpenProse-native campaign for turning the OATS listing target research report
into draft GitHub pull requests and a review report. This is intentionally a
contract, not a JavaScript batch runner: the host embodies the OpenProse VM,
fans out workers, adapts to each target repository, and records what happened.

### Parameters

- `research_report_path`: path to the source OATS listing target research report.
  Default: `docs/outreach/oats-listing-target-research.md`.
- `target_manifest_path`: path to the structured target manifest derived from
  the report. Default: `docs/outreach/listing-targets.json`.
- `oats_profile_path`: path to the canonical OATS listing profile. Default:
  `docs/outreach/oats-listing-profile.md`.
- `public_campaign_report_url`: URL to include in external pull request bodies.
  Default: `https://github.com/ariso-ai/oats/blob/main/docs/outreach/listing-pr-batch-report.md`.
- `phase`: optional campaign phase filter such as `exact-fit-pr-sprint`.
- `target_selectors`: optional list of target ids, names, or `owner/repo` values.
- `limit`: optional maximum number of selected GitHub list targets.
- `dry_run`: boolean. Default: `true`. When true, do not fork, push, or create
  pull requests; write a plan report only.
- `draft_mode`: boolean. Default: `true`. Create draft pull requests when the
  host is in live mode and GitHub supports drafts.
- `review_report_path`: path for the local operator report that links draft PRs.
  Default: `.outreach/listing-pr-runs/{run_id}/review.md`.

### Returns

- `selected_targets`: GitHub list targets chosen from the research report and
  manifest, with placement hypotheses and fit reasons.
- `deferred_targets`: report targets that are useful but not handled by GitHub
  list PR workers in this run, grouped by topic, form, manual, and package work.
- `pull_requests`: draft pull request receipts, each with target repo, branch,
  URL, draft status, placement, changed files, and human PR description.
- `skipped_targets`: targets the workers inspected but did not post to, with the
  concrete blocker.
- `review_report`: local Markdown report path and summary linking every created
  draft PR.

### Errors

- `source_report_unavailable`: the research report or manifest cannot be read.
- `github_auth_unavailable`: live mode was requested but GitHub authentication
  or required GitHub tooling is unavailable.
- `host_cannot_spawn_workers`: live mode was requested in a host that cannot
  spawn isolated sessions for the target workers.

### Invariants

- `dry_run` is the default. Forking, pushing, and creating pull requests require
  an explicit caller override.
- Every external pull request is draft by default and links the public campaign
  report.
- Worker scratch stays in `.outreach/` or the OpenProse run workspace; no worker
  commits generated run state into this repository.
- Each target worker receives only the single target, the OATS profile, the
  source-report excerpt or manifest row that justifies the attempt, and the
  campaign report URL.
- Each target worker must inspect the target repository's current README,
  contribution guidance, existing entry style, and open pull requests before
  editing.
- Each target worker creates the smallest acceptable listing edit and leaves the
  target repository unchanged when no honest placement exists.
- Pull request descriptions must sound like a maintainer-to-maintainer note:
  concise, specific, factual, and free of automation boilerplate.

### Strategies

- Be liberal when selecting GitHub list targets after the exact-fit split: if
  OATS plausibly belongs in a Mac, menu bar, note-taking, transcription,
  privacy, local-first, local-AI, Tauri, Rust, Vue, or open-source alternatives
  list, include it unless the target's stated rules clearly exclude apps like
  OATS.
- Prioritize high-likelihood and high-fit targets first, but keep lower-likelihood
  list targets visible as deferred or inspect-and-skip results rather than
  silently dropping them.
- Prefer existing list language over a fixed OATS blurb. Match section names,
  link style, punctuation, sort order, description length, and contribution
  templates from the target repository.
- Use the repository URL `https://github.com/ariso-ai/oats` as the canonical link
  unless a target explicitly prefers a website or app-store-style URL.
- For draft PR bodies, write the short human version: what changed, why OATS fits
  that list, and that the edit was kept to one listing entry.
- If draft PR creation fails only because drafts are unsupported, retry as ready
  only when the caller explicitly set `draft_mode` to false; otherwise record the
  blocker and continue.
- Record topic updates, forms, editorial submissions, and package PRs as follow-up
  lanes in the final report; do not pretend they are GitHub list PRs.

### Environment

- `GH_TOKEN`: optional. Used only by host GitHub tooling when live mode creates
  forks, pushes branches, or opens pull requests.

### Tools

- `cli:git`: clone, branch, diff, commit, and push target repositories in live
  mode.
- `cli:gh`: inspect GitHub repositories and create draft pull requests in live
  mode.

### Shape

- `self`: read the research report and manifest, select postable GitHub list
  targets, coordinate worker fan-out, merge worker receipts, and publish the
  local review report.
- `delegates`: target selection workers, one listing publisher worker per target,
  and a report synthesizer.
- `prohibited`: direct directory form submissions, GitHub topic edits, package
  index packaging work, posting non-draft PRs unless explicitly requested, or
  opening pull requests without recording a review report.

### Execution

```prose
let preflight = call preflight_campaign
  target_manifest_path: target_manifest_path
  research_report_path: research_report_path
  oats_profile_path: oats_profile_path
  dry_run: dry_run

let target_plan = call select_github_list_targets
  research_report_path: research_report_path
  target_manifest_path: target_manifest_path
  phase: phase
  target_selectors: target_selectors
  limit: limit
  liberal_selection: true

if dry_run:
  let dry_report = call write_campaign_report
    mode: "dry-run"
    selected_targets: target_plan.selected_targets
    deferred_targets: target_plan.deferred_targets
    pull_requests: []
    skipped_targets: []
    review_report_path: review_report_path
    public_campaign_report_url: public_campaign_report_url

  return {
    selected_targets: target_plan.selected_targets,
    deferred_targets: target_plan.deferred_targets,
    pull_requests: [],
    skipped_targets: [],
    review_report: dry_report
  }

if preflight live mode is not ready:
  throw "github_auth_unavailable"

let worker_receipts = target_plan.selected_targets
  | pmap:
      call publish_listing_target
        target: item
        oats_profile_path: oats_profile_path
        public_campaign_report_url: public_campaign_report_url
        draft_mode: draft_mode

let worker_results = call merge_worker_results
  worker_receipts: worker_receipts

let live_report = call write_campaign_report
  mode: "live"
  selected_targets: target_plan.selected_targets
  deferred_targets: target_plan.deferred_targets
  pull_requests: worker_results.pull_requests
  skipped_targets: worker_results.skipped_targets
  review_report_path: review_report_path
  public_campaign_report_url: public_campaign_report_url

return {
  selected_targets: target_plan.selected_targets,
  deferred_targets: target_plan.deferred_targets,
  pull_requests: worker_results.pull_requests,
  skipped_targets: worker_results.skipped_targets,
  review_report: live_report
}
```

## preflight_campaign

### Parameters

- `target_manifest_path`: path to the structured target manifest.
- `research_report_path`: path to the source research report.
- `oats_profile_path`: path to the OATS listing profile.
- `dry_run`: boolean indicating whether live GitHub side effects are disabled.

### Returns

- `status`: `ready`, `dry-run-ready`, or `blocked`.
- `notes`: concrete checks performed and any missing live-mode capability.

### Strategies

- In dry-run mode, require only readable local input files and a writable report
  destination.
- In live mode, require readable inputs, `git`, `gh`, authenticated GitHub
  access, push capability to forks, and a host that can spawn isolated sessions.
- Check only secret presence, never the value.

## select_github_list_targets

### Parameters

- `research_report_path`: source research report to treat as the authority for
  target provenance and fit rationale.
- `target_manifest_path`: structured manifest derived from the research report.
- `phase`: optional campaign phase filter.
- `target_selectors`: optional ids, names, or `owner/repo` values.
- `limit`: optional maximum target count.
- `liberal_selection`: boolean; when true, include plausible post-split list
  targets rather than only exact matches.

### Returns

- `selected_targets`: ordered GitHub list PR targets. Each entry includes id,
  name, repo, URL, phase, priority, report evidence, fit rationale, placement
  hypothesis, and worker prompt context.
- `deferred_targets`: non-list-PR targets and any GitHub rows intentionally held
  for another lane, grouped with next action.

### Strategies

- Read the source report first, then use the manifest for structure; if they
  disagree, preserve the report's rationale and mark the manifest issue.
- Select only targets whose submission kind is `github-pr` for publisher workers.
- Include `package-pr` targets only in deferred packaging lanes.
- Include `github-topic`, `form`, and `manual` targets only in deferred follow-up
  lanes.
- Sort by priority, then high acceptance likelihood, then exact audience fit.
- When `phase` or `target_selectors` is provided, filter after the report/manifest
  consistency pass.
- When `limit` is provided, still count and summarize the omitted eligible targets.

### Execution

```prose
agent target_scout:
  model: sonnet
  persist: project
  prompt: """
    You filter OATS listing targets from the source research report.

    Be liberal after the exact-fit split: keep GitHub list repos when OATS
    plausibly belongs as a Mac app, menu bar app, note-taking app, transcription
    tool, privacy/local-first tool, local-AI app, Tauri app, Rust/Vue app, or
    open-source alternative.

    Return structured target rows, not prose-only notes. Preserve the report's
    rationale and URL for each decision.
  """
  shape:
    self: ["source-report filtering", "fit scoring", "deferred-lane grouping"]
    prohibited: ["posting PRs", "editing repositories"]

parallel:
  let exact_fit_targets = session exact-fit-scout: target_scout
    prompt: """
      From the source report and manifest, select the exact-fit GitHub list PR
      targets for Mac apps, menu bar apps, note-taking, meeting notes,
      transcription, Whisper/ASR, local-first, privacy, and OATS alternatives.
      Apply the phase, target selector, and limit only after preserving the full
      eligible count.
    """
    context: { research_report_path, target_manifest_path, phase, target_selectors, limit }

  let ecosystem_targets = session ecosystem-scout: target_scout
    prompt: """
      From the source report and manifest, select plausible ecosystem GitHub list
      PR targets for Tauri, Rust, Vue, local AI, open-source apps, and broader
      productivity or alternatives lists. Be liberal, but attach a fit caveat
      when acceptance is medium or low.
    """
    context: { research_report_path, target_manifest_path, phase, target_selectors, limit }

  let deferred_lanes = session deferred-lane-scout: target_scout
    prompt: """
      From the source report and manifest, group all GitHub topic, form, manual,
      and package-PR targets into follow-up lanes. Do not include them in the
      publisher-worker set.
    """
    context: { research_report_path, target_manifest_path, phase, target_selectors, limit }

let merged_plan = session target-plan-merge: target_scout
  prompt: """
    Merge the scout outputs into one ordered campaign plan.
    Deduplicate targets by GitHub repo, keep the most specific fit rationale,
    sort by manifest priority and acceptance likelihood, then apply the caller
    limit. Return selected_targets and deferred_targets.
  """
  context: { exact_fit_targets, ecosystem_targets, deferred_lanes, liberal_selection }

return merged_plan
```

## publish_listing_target

### Parameters

- `target`: one selected GitHub list target with report evidence and manifest
  metadata.
- `oats_profile_path`: path to the canonical OATS listing profile.
- `public_campaign_report_url`: public report URL to link from the pull request.
- `draft_mode`: boolean indicating whether to create the pull request as draft.

### Returns

- `pull_requests`: zero or one draft PR receipt for the target.
- `skipped_targets`: zero or one skip receipt for the target.

### Invariants

- The worker may edit only the cloned target repository, not this OATS repository.
- The worker must not commit if it cannot name the exact section and entry format
  that make OATS fit.
- The commit message must be a valid Conventional Commit, preferably
  `docs: add oats`.
- The worker must record the exact changed files and the final entry text.

### Shape

- `self`: inspect one target repository, make one minimal listing edit, and open
  one draft PR when appropriate.
- `prohibited`: broad rewrites, adding badges/screenshots, modifying unrelated
  sections, inventing claims about OATS, or opening multiple PRs for one target.

### Execution

```prose
agent listing_publisher:
  model: sonnet
  persist: project
  prompt: """
    You are adding OATS to one public curated GitHub list.

    Work like a human maintainer would:
    - Inspect the current target repository before editing.
    - Read the README, contribution guide, list sections, examples, and open pull
      requests that may affect placement.
    - Add one OATS entry in the most specific honest section.
    - Match the target list's existing ordering, punctuation, link style, and
      description length.
    - Prefer the OATS GitHub URL unless the target requires a website URL.
    - Use a factual short description from the OATS profile.
    - If the list rules exclude OATS or no good section exists, do not edit; return
      a skip receipt with the exact blocker.
    - If you edit, commit with a Conventional Commit and create a concise draft PR
      body that links the public campaign report.
  """
  shape:
    self: ["repository inspection", "minimal list editing", "draft PR creation"]
    prohibited: ["editing this OATS repo", "rewriting unrelated list content", "marketing copy"]

let placement = session target-placement: listing_publisher
  prompt: """
    Inspect the target repository and decide whether OATS belongs.
    Return: target repo, file path, section heading, ordering rule, exact entry
    format, contribution constraints, and a yes/no placement decision.
  """
  context: { target, oats_profile_path, public_campaign_report_url }

if placement says OATS should not be posted:
  return {
    pull_requests: [],
    skipped_targets: [placement]
  }

let publication = session target-publication: listing_publisher
  prompt: """
    Make the smallest acceptable listing edit for OATS, commit it, push a branch,
    and open the pull request.

    Pull request body requirements:
    - one short summary sentence
    - one short fit paragraph specific to this target list
    - link the public campaign report
    - no automation disclosure unless the target requires it
    - draft mode if supported and requested

    Return the PR URL, draft status, branch, changed files, section used, final
    entry text, commit message, and any caveats.
  """
  context: { target, placement, oats_profile_path, public_campaign_report_url, draft_mode }

return publication
```

## merge_worker_results

### Parameters

- `worker_receipts`: ordered collection returned by `publish_listing_target`
  workers.

### Returns

- `pull_requests`: flattened PR receipt collection.
- `skipped_targets`: flattened skip receipt collection.
- `failures`: worker failures that did not produce either receipt shape.

### Strategies

- Preserve input target order for successful and skipped receipts.
- Treat malformed receipts as failures with the target id if it can be recovered.
- Do not drop failures; the final report must make them reviewable.

## write_campaign_report

### Parameters

- `mode`: `dry-run` or `live`.
- `selected_targets`: targets selected for publisher workers.
- `deferred_targets`: non-PR or held targets grouped by follow-up lane.
- `pull_requests`: pull request receipts returned by workers.
- `skipped_targets`: skip receipts returned by workers.
- `review_report_path`: local report path requested by the caller.
- `public_campaign_report_url`: public campaign report URL linked from PRs.

### Returns

- `review_report`: local Markdown report path, headline counts, draft PR links,
  skipped targets, deferred lanes, and next recommended run.

### Strategies

- The report is for the OATS maintainer to review, so lead with draft PR links
  and blockers rather than implementation details.
- Include enough context for each PR to decide whether to review, revise, or close:
  target, section, final entry text, worker caveats, and PR URL.
- In dry-run mode, include target placement hypotheses and exact live-mode command
  prompt, but do not imply PRs were created.
- Keep the public campaign report URL separate from the local operator report:
  external PRs link the public report, while the operator report links the draft
  PRs.
