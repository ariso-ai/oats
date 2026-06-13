# Release Pipeline Consolidation — Design

**Date:** 2026-06-13
**Status:** Approved for planning

## Problem

The `release` workflow (`.github/workflows/release.yaml`) fails on push to `main`:

```
##[error]release-please failed: Input required and not supplied: token
```

The job passes `token: ${{ secrets.RELEASE_PLEASE_TOKEN }}`, but that secret does
not exist (verified: 0 repo-level secrets; the `release` environment holds 8
secrets, none of them `RELEASE_PLEASE_TOKEN`). GitHub substitutes an empty
string, so the required `token` input is empty and the action aborts.

The workflow deliberately requires a PAT/App token rather than the default
`GITHUB_TOKEN` for two reasons, both rooted in the rule that *events authored by
`GITHUB_TOKEN` do not trigger other workflow runs*:

- **Use A — publish trigger:** release-please publishes a GitHub Release whose
  `release: published` event must trigger `desktop.yaml`'s `release`/`publish`
  jobs.
- **Use B — lockfile push:** the `sync-lock` job pushes regenerated lockfiles
  onto the release-PR branch and (today) relies on that push re-triggering
  `desktop.yaml`'s `validate` job on the PR.

Rather than provision and rotate a PAT (which expires) or set up a GitHub App,
we eliminate the need for a non-default token by removing the cross-workflow
event dependency entirely.

## Key facts that shape the design

- **`main` is not a protected branch** — there are no required status checks
  (verified via the branch-protection API → 404). So Use B's re-trigger has no
  merge-gating value today; its only cost is a stale PR check.
- **release-please config** uses `release-type: node` (bumps `package.json` and
  `package-lock.json` version fields) plus generic `toml`/`json` updaters for
  `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`. It does **not** touch
  `src-tauri/Cargo.lock` — that is the real reason `sync-lock` exists, because
  the release/validate builds run `cargo build --locked`.

## Approach

Consolidate the entire release pipeline into `release.yaml` so the GitHub
Release and the desktop build run in **one workflow run**. With no separate
triggered workflow, `GITHUB_TOKEN` is sufficient everywhere — it can create the
Release; it only ever lacked the ability to trigger *other* workflows, which no
longer happens.

`desktop.yaml` is reduced to its CI role: the `validate` job on `pull_request`
and `push: main`. Its `release: [published]` trigger and its `release`/`publish`
jobs are removed (moved into `release.yaml`).

### `release.yaml` (push to `main`)

Workflow-level concurrency stays `group: ${{ github.workflow }}`,
`cancel-in-progress: false` (queue, never cancel an in-flight signing/notarizing
build).

1. **`release-please`**
   - `token: ${{ secrets.GITHUB_TOKEN }}` (the fix; no PAT).
   - Same config/manifest/target-branch as today.
   - Outputs: `releases_created`, `prs_created`, `prs`, `tag_name`.
   - `permissions: contents: write, pull-requests: write`.

2. **`sync-lock`** — gated on `needs.release-please.outputs.prs_created == 'true'`
   - Checkout the release-PR branch with the default `GITHUB_TOKEN`.
   - Set up Node 24 + stable Rust.
   - `npm install --package-lock-only --ignore-scripts`.
   - `cargo update --workspace` in `src-tauri`.
   - Commit `package-lock.json` + `src-tauri/Cargo.lock` (if changed) and push
     with `GITHUB_TOKEN`. **No in-job validation** (the chosen approach): the
     job stays lean and compiles nothing, exactly as today. The push will not
     re-trigger the PR's `validate` check, which is acceptable since `main` has
     no required checks; correctness of the synced lockfiles is caught by the
     `release` job's `cargo build --locked` after the PR merges.
   - `permissions: contents: write`.

3. **`release`** (moved from `desktop.yaml`) — gated on
   `needs.release-please.outputs.releases_created == 'true'`
   - `needs: release-please`, `environment: release`, `permissions: contents: read`.
   - Checkout `needs.release-please.outputs.tag_name`.
   - Identical build steps to today's `desktop.yaml` `release` job: Node, Rust,
     cargo cache, sidecar cache + build, `npm ci`, Apple/Tauri signing env,
     `npm run tauri:build -- -- --features prod-api`, upload `release-bundle`
     artifact.

4. **`publish`** (moved from `desktop.yaml`) — `needs: release`
   - `environment: release`, `permissions: contents: write`.
   - Checkout `needs.release-please.outputs.tag_name`, download `release-bundle`.
   - The release `body`/`name` are no longer available via `github.event.release.*`;
     fetch them with `gh release view "$TAG" --json body,name` (gh uses
     `GITHUB_TOKEN`). `RELEASE_TAG` comes from the `tag_name` output.
   - Run `.github/scripts/release-publish.sh`, then append the R2 download link
     to the GitHub Release (`softprops/action-gh-release@v2`).

### `desktop.yaml`

- Remove the `release: [published]` trigger (keep `pull_request` + `push: main`).
- Remove the `release` and `publish` jobs.
- Keep the `validate` job unchanged.

## Run-flow walkthrough

- **Feature commit lands on `main`:** release-please opens/updates the release PR
  (`prs_created=true`) → `sync-lock` runs (syncs + pushes lockfiles).
  `release`/`publish` skipped.
- **Release PR merged to `main`:** the merge is a push to `main` → release-please
  detects the merged PR and creates the GitHub Release + tag
  (`releases_created=true`) → `release` builds/signs → `publish` ships to R2.
  `sync-lock` skipped.

## Why `GITHUB_TOKEN` is now sufficient

- Creating the GitHub Release: `GITHUB_TOKEN` with `contents: write` can do this.
- Triggering the build: no longer relies on an event — the build is a downstream
  job in the same run, gated on `releases_created`.
- The lockfile push: `GITHUB_TOKEN` push is fine; `main` has no required checks
  to keep green, and the synced lockfiles are exercised by the `release` job's
  `cargo build --locked` after merge.

## Out of scope / non-goals

- Making release-please own `Cargo.lock` (the "drop sync-lock" idea) — rejected
  for now: TOML array-of-tables targeting is fiddly and needs separate testing.
- Any change to signing, notarization, R2 publishing logic, or
  `release-publish.sh`.
- Branch protection / required status checks on `main`.

## Risks

- **Unvalidated lockfile on the PR:** `sync-lock` pushes without building, and
  the push does not re-trigger the PR's `validate` check, so the release PR's
  checks reflect the pre-sync commit. A broken lockfile would therefore only
  surface in the `release` job's `cargo build --locked` after the PR is merged.
  Accepted trade-off: `cargo update --workspace` for a version-only bump is
  deterministic and low-risk.
- **release-please output coverage:** confirm v4 exposes `tag_name` and
  `releases_created` as top-level outputs (single package, `include-component-in-tag:
  false`). The `body`/`name` are fetched via `gh release view` rather than
  assumed to be action outputs.
