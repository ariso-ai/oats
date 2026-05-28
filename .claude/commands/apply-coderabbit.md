---
description: Fetch unresolved CodeRabbit findings on the current PR, apply the clearly-correct ones with judgment, build, and commit.
---

Fetch all unresolved CodeRabbit review comments on the current PR and apply
the ones that are clearly correct.

This repo is a Tauri app: React + TypeScript frontend in `src/`, Rust
backend in `src-tauri/`. Build verification uses `npm run vite:build`
(frontend) and `cargo build --locked` from `src-tauri/` (Rust). There is
no test suite yet.

## Step 1 — Identify the PR

```bash
PR=$(gh pr view --json number --jq .number)
REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
OWNER="${REPO%%/*}"
NAME="${REPO##*/}"
echo "PR #$PR on $REPO"
```

If `gh pr view` fails, the current branch has no open PR — stop and tell
the user.

## Step 2 — Fetch CodeRabbit findings

CodeRabbit's actionable findings live in threaded **inline review
comments**, not in PR-level summary comments. Pull the review threads
via GraphQL so we also get `isResolved` status, and filter by the exact
bot identity (`coderabbitai[bot]`). Username-prefix matching is unsafe
— anyone could register `coderabbit-junior`.

```bash
gh api graphql -F owner="$OWNER" -F name="$NAME" -F number="$PR" -f query='
  query($owner: String!, $name: String!, $number: Int!) {
    repository(owner: $owner, name: $name) {
      pullRequest(number: $number) {
        reviewThreads(first: 100) {
          nodes {
            id
            isResolved
            path
            line
            comments(first: 50) {
              nodes {
                id
                databaseId
                author { login }
                body
              }
            }
          }
        }
      }
    }
  }
' | jq '
  [ .data.repository.pullRequest.reviewThreads.nodes[]
    | select(.isResolved == false)
    # GraphQL Actor.login strips the [bot] suffix that REST includes,
    # so compare against the bare bot login here.
    | select((.comments.nodes[0].author.login // "") == "coderabbitai")
    | {threadId: .id, path, line,
       rootCommentId: .comments.nodes[0].databaseId,
       body: .comments.nodes[0].body}
  ]
' > /tmp/coderabbit-findings.json

echo "Unresolved CodeRabbit findings: $(jq length /tmp/coderabbit-findings.json)"
```

Also pull the PR-level summary comments from CodeRabbit (the "walkthrough"
post) for context — usually informational, not actionable:

```bash
gh api "repos/$REPO/issues/$PR/comments" --paginate \
  | jq -s 'add // [] | [.[] | select(.user.login == "coderabbitai[bot]" and .user.type == "Bot") | {body}]' \
  > /tmp/coderabbit-summary.json
```

## Step 3 — Triage each finding

For each unresolved finding in `/tmp/coderabbit-findings.json`, decide:

- **Apply as-is** — clear bug fix, typo, obvious correctness issue, or a
  small refactor that matches existing patterns in the codebase.
- **Apply with modification** — the underlying point is right but the
  suggested implementation doesn't fit this codebase (wrong style, wrong
  API, doesn't account for caller behavior). Write a better version.
- **Skip with reason** — false positive, conflicts with project
  conventions, or requires a design discussion that shouldn't happen in
  a one-shot edit. Note the reason; it goes in the commit message and
  the PR reply.

Read the diff and the surrounding file context before deciding. Use
`Read`, `Grep`, and `git diff origin/<base>...HEAD -- <path>` as needed.

### Do NOT apply suggestions that…

- Remove error handling (Rust `?` / `match` over `Result`, JS `try`/
  `catch`, Tauri command error returns). If a finding asks to swap `?`
  for `.unwrap()` or to drop a catch, skip it.
- Change public API signatures without confirming callers via `grep`:
  - Tauri commands (`#[command]` in `src-tauri/src/commands.rs`) —
    callers are in the React frontend via `invoke()`.
  - Exported TypeScript symbols — callers may be anywhere in `src/`.
  - Public Rust items (`pub fn`, `pub struct`) that cross modules.
- Touch concurrency primitives without explicit reasoning:
  - Rust async (`tokio::spawn`, `Arc<Mutex>`, channels), audio capture
    threading in `src-tauri/src/audio_capture.rs`.
  - React effect dependency arrays and effect cleanup ordering.
  - IPC contracts between the React frontend and Rust backend (event
    names, payload shapes, command schemas).
- Modify `tauri.conf.json` capabilities, `Info.plist`, or signing
  config — these affect notarization and gate.
- Disagree with patterns established elsewhere in the codebase. If a
  finding asks for a pattern the rest of the repo doesn't use, skip and
  note the reason.

## Step 4 — Build verification

Run both builds and confirm they succeed before committing. Do not
suppress warnings.

```bash
npm run vite:build
( cd src-tauri && cargo build --locked )
```

If either fails, stop and resolve the failure before continuing. If a
finding caused the failure, back it out and mark it as skipped with the
build error as the reason.

## Step 5 — Commit

Group all applied changes into one commit:

```
git add -A
git commit -m "Apply CodeRabbit suggestions

Applied:
  - <file>:<line> — <one-line summary>
  - ...

Skipped:
  - <file>:<line> — <reason>
  - ..."
```

If `commit.gpgsign` is set in this repo's config, **do not** bypass it
(no `--no-gpg-sign`, no `-c commit.gpgsign=false`). If signing fails,
investigate rather than disabling it.

## Step 6 — Reply on the PR

For each thread you addressed, post a reply that asks CodeRabbit to
resolve. CodeRabbit listens for `@coderabbitai resolve` in a thread
reply.

```bash
# For each applied finding, reply on the specific review thread:
gh api "repos/$REPO/pulls/$PR/comments/<root_comment_databaseId>/replies" \
  -f body="Applied in <short-sha>. @coderabbitai resolve"
```

(Use the `rootCommentId` from `/tmp/coderabbit-findings.json` as
`<root_comment_databaseId>`.)

For each finding you skipped, post a brief reply explaining why so the
reviewer can decide whether to escalate:

```bash
gh api "repos/$REPO/pulls/$PR/comments/<root_comment_databaseId>/replies" \
  -f body="Skipping: <one-sentence reason>."
```

Do **not** post `@coderabbitai resolve` on threads you skipped.

## Step 7 — Push

Push the new commit so the `PR Review` workflow re-runs against the
updated diff:

```bash
git push
```

The workflow will re-evaluate. Any CodeRabbit findings you didn't
address (and didn't get the thread marked resolved) will still count
toward the merge gate; if all remaining items are intentional, you or
another resolver can mark threads resolved in the UI or add the
`override-ai-review` label.

## Cleanup

```bash
rm -f /tmp/coderabbit-findings.json /tmp/coderabbit-summary.json
```
