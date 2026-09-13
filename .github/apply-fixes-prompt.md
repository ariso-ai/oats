You are running inside a GitHub Actions job. CodeRabbit is the merge
gate — your job is to apply the major/critical findings that CodeRabbit
and shawnzhu have raised on this PR, plus any minor CodeRabbit findings
that are genuine true positives, so the next CodeRabbit re-review can
clear them.

This repo is a Tauri app: Vue 3 + TypeScript in `src/`, Rust backend in
`src-tauri/`. The frontend has a Vitest suite. Verification is:

```bash
npm test
npm run vite:build
( cd src-tauri && cargo build --locked )
```

## Inputs

A sibling CI step has written `actionable-findings.json` to the runner
temp directory; the absolute path is appended to this prompt. The file
has this shape:

```json
{
  "coderabbit": [
    {"threadId": "...", "rootCommentId": 123, "path": "...", "line": 42,
     "body": "..."}
  ],
  "coderabbit_minor": [
    {"threadId": "...", "rootCommentId": 789, "path": "...", "line": 8,
     "body": "..."}
  ],
  "shawnzhu": [
    {"threadId": "...", "rootCommentId": 456, "path": "...", "line": 17,
     "body": "...", "replies": [{"author": "...", "body": "..."}]}
  ]
}
```

`coderabbit` entries are unresolved review threads where the root
comment is from `coderabbitai[bot]` and matches the major-finding
pattern (`🔴 Critical`, `🟠 Major`, `🛑`, security/data-loss/race-condition).

`coderabbit_minor` entries are unresolved CodeRabbit threads carrying
the `🟡 Minor` badge (and not matching the major pattern). These are
held to a stricter bar — see "Minor findings" below.

`shawnzhu` entries are unresolved review threads where the root comment
is from `shawnzhu` — all of them, no severity filter. Treat each as
actionable unless triage says otherwise.

Every entry also carries `replies` — the thread's later comments, oldest
first, as `{"author", "body"}`. Read them: a reviewer may already have
explained why a finding is intended, or narrowed what they want.
Threads this workflow (`github-actions`) already replied to are
excluded, unless a human reviewer followed up after that reply — in
that case the follow-up overrides the earlier triage; act on it.

## Step 1 — Triage each finding

For each entry in all three arrays, classify:

- **valid-unaddressed** — real issue, not yet fixed in this PR. **Apply
  a fix.**
- **valid-already-fixed** — real issue, but the current diff already
  addresses it. **Skip** — note this in the commit body and reply on
  the thread.
- **false-positive** — false alarm: the defect isn't there, can't be
  reached, or the suggestion goes against project conventions.
  **Skip** with a one-sentence reason. The workflow resolves the thread.
- **needs-human** — may well be real, but this job can't or shouldn't
  fix it: it needs a design discussion, a change outside the repo
  (runner or infra setup), or an edit this job is barred from (see
  "Do NOT apply" and "Important" below). **Skip** with a one-sentence
  reason. The thread stays open for a human.

Pick `false-positive` only when you are confident the finding is wrong.
If it might be right and you just can't act on it, it is `needs-human`.

Read the diff and surrounding file context before deciding. Use `Read`,
`Grep`, and `git diff origin/<base>...HEAD -- <path>` as needed.

### Minor findings (`coderabbit_minor`)

The default for a minor finding is **skip**. Every edit costs a new
commit and a re-review cycle, and CodeRabbit's minors are often wrong
or not worth it. Classify a minor as `valid-unaddressed` only if you
can answer "yes" to all of these:

1. **It is a true positive.** You traced the actual code path — not
   just the snippet CodeRabbit quoted — and confirmed the defect
   exists: wrong result, crash/panic, unhandled error, leak, a race you
   can describe step by step, or a test that asserts the wrong thing or
   can't fail.
2. **It is reachable here.** The triggering input or state can
   actually occur in this app. Check callers and invariants; a guard
   for a case the code already rules out is not a fix.
3. **It is introduced or touched by this PR.** Check
   `git diff origin/<base>...HEAD`. Pre-existing issues on lines the
   PR doesn't touch are out of scope.
4. **The fix is small and local.** A few lines in the flagged area,
   no new abstractions, no behavior change beyond the defect.

Skip (classify as `false-positive`) minors that are:

- Style, naming, formatting, readability, or "consider…" refactors.
- Hypothetical edge cases, defensive checks for already-guaranteed
  invariants, or "for robustness" additions.
- Extra logging, comments, docs, or tests added only for coverage.
- Correct in general but based on a wrong reading of this code (for
  example, CodeRabbit missed a guard, caller, or invariant elsewhere).

When unsure, skip — and say in the reply what you checked, e.g.
"Skipping (minor, not a true positive): `foo` is only called after
`bar` validates the input." A minor that fails any of the four checks
is a `false-positive`, even when it isn't strictly wrong — minors are
never `needs-human`, so a skipped minor gets resolved rather than left
open. Majors and `shawnzhu` findings keep the normal bar above.

## Step 2 — Apply fixes

Edit files to apply each `valid-unaddressed` finding. Group related
edits together. Use the `Edit` tool — no shell-based file rewriting.

### Do NOT apply suggestions that

- Touch documentation-only files — anything under `docs/` or any `*.md`
  file. Code is the source of truth; doc drift is out of scope for this
  job and is never worth a commit here.
- Remove error handling (Rust `?` / `match` over `Result`, JS `try`/
  `catch`, Tauri command error returns).
- Change public API signatures without confirming callers via `grep`:
  - Tauri commands (`#[command]` in `src-tauri/src/commands.rs`) —
    callers are in the Vue frontend via `invoke()`.
  - Exported TypeScript symbols.
  - Public Rust items (`pub fn`, `pub struct`) that cross modules.
- Touch concurrency primitives (`tokio::spawn`, `Arc<Mutex>`, channels,
  React effect deps, IPC contracts) without explicit reasoning.
- Modify `tauri.conf.json` capabilities, `Info.plist`, or signing
  config — these affect notarization.
- Disagree with patterns established elsewhere in the codebase.

If a suggestion conflicts with one of these rules, classify it as
**needs-human** with the conflicting rule as the reason — the finding
may be right, but a human has to make that change. The exception is a
suggestion that goes against patterns established elsewhere: that is a
**false-positive**.

## Step 3 — Build verification

Run all three. Do not suppress warnings.

```bash
npm test
npm run vite:build
( cd src-tauri && cargo build --locked )
```

If any fails, isolate the failure to a specific finding's edit, back
it out, and reclassify that finding as `needs-human` with the build
error as the reason. Re-run until they pass cleanly.

If you cannot make them pass, stop. Do not commit. Write a clear
summary of what failed.

## Step 4 — Commit & reply

If you applied at least one fix, the workflow will handle the commit
and push using `github-actions[bot]` identity. Write your commit
message to `/tmp/commit-message.txt` with this structure:

```
Apply review suggestions from CodeRabbit and shawnzhu

Applied:
  - <path>:<line> — <one-line summary of the change>

Skipped (false-positive):
  - <path>:<line> — <reason>

Skipped (needs-human):
  - <path>:<line> — <reason>

Skipped (already-fixed):
  - <path>:<line>
```

Then write a JSON array to `/tmp/thread-replies.json` describing the
replies to post — one per finding across all three arrays, skipped
minors included. That reply is the triage record: the next run skips
any thread that already has one.

```json
[
  {"rootCommentId": 123, "kind": "applied",
   "body": "Applied: <one-line summary>."},
  {"rootCommentId": 789, "kind": "already-fixed",
   "body": "Already addressed in <short-sha>."},
  {"rootCommentId": 456, "kind": "false-positive",
   "body": "Skipping (false positive): <one-line reason>."},
  {"rootCommentId": 321, "kind": "needs-human",
   "body": "Leaving open for a human: <one-line reason>."}
]
```

`kind` must be exactly one of those four values. After the push, the
workflow posts each reply, then resolves the CodeRabbit threads whose
kind is `applied`, `already-fixed`, or `false-positive`. `needs-human`
threads, and every `shawnzhu` thread, stay open. Do not write
`@coderabbitai resolve` in a body — the workflow resolves threads
itself.

## Step 5 — If nothing to apply

If every finding triages to `false-positive`, `needs-human`, or
`already-fixed` — i.e., you have no edits to make — do not write a
commit message.
Still write `/tmp/thread-replies.json` so the workflow can post
explanations on each thread. The workflow detects "no changes" and
skips the commit step.

## Important

- Do not push or commit yourself. The workflow does that.
- Do not modify `.github/workflows/` files — those are the workflow
  driving you, and changes there should come from a human.
- Do not modify `.coderabbit.yaml` — same reason.
- Stay focused on the findings you were given. Do not roam the codebase
  fixing unrelated issues.
