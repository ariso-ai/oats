You are running inside a GitHub Actions job. CodeRabbit is the merge
gate — your job is to apply the major/critical findings that CodeRabbit
and shawnzhu have raised on this PR, so the next CodeRabbit re-review
can clear them.

This repo is a Tauri app: React + TypeScript in `src/`, Rust backend in
`src-tauri/`. There is no test suite. Build verification is:

```bash
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
  "shawnzhu": [
    {"threadId": "...", "rootCommentId": 456, "path": "...", "line": 17,
     "body": "..."}
  ]
}
```

`coderabbit` entries are unresolved review threads where the root
comment is from `coderabbitai[bot]` and matches the major-finding
pattern (`⚠️ Potential issue`, `🛑`, security/data-loss/race-condition).

`shawnzhu` entries are unresolved review threads where the root comment
is from `shawnzhu` — all of them, no severity filter. Treat each as
actionable unless triage says otherwise.

## Step 1 — Triage each finding

For each entry in both arrays, classify:

- **valid-unaddressed** — real issue, not yet fixed in this PR. **Apply
  a fix.**
- **valid-already-fixed** — real issue, but the current diff already
  addresses it. **Skip** — note this in the commit body and reply on
  the thread.
- **false-positive** — false alarm, conflicts with project conventions,
  or requires a design discussion. **Skip** with a one-sentence reason.

Read the diff and surrounding file context before deciding. Use `Read`,
`Grep`, and `git diff origin/<base>...HEAD -- <path>` as needed.

## Step 2 — Apply fixes

Edit files to apply each `valid-unaddressed` finding. Group related
edits together. Use the `Edit` tool — no shell-based file rewriting.

### Do NOT apply suggestions that

- Remove error handling (Rust `?` / `match` over `Result`, JS `try`/
  `catch`, Tauri command error returns).
- Change public API signatures without confirming callers via `grep`:
  - Tauri commands (`#[command]` in `src-tauri/src/commands.rs`) —
    callers are in the React frontend via `invoke()`.
  - Exported TypeScript symbols.
  - Public Rust items (`pub fn`, `pub struct`) that cross modules.
- Touch concurrency primitives (`tokio::spawn`, `Arc<Mutex>`, channels,
  React effect deps, IPC contracts) without explicit reasoning.
- Modify `tauri.conf.json` capabilities, `Info.plist`, or signing
  config — these affect notarization.
- Disagree with patterns established elsewhere in the codebase.

If a suggestion conflicts with one of these rules, classify it as
**false-positive** with the conflicting rule as the reason.

## Step 3 — Build verification

Run both builds. Do not suppress warnings.

```bash
npm run vite:build
( cd src-tauri && cargo build --locked )
```

If either fails, isolate the failure to a specific finding's edit, back
it out, and reclassify that finding as `false-positive` with the build
error as the reason. Re-run the builds until they pass cleanly.

If you cannot make the builds pass, stop. Do not commit. Write a clear
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

Skipped (already-fixed):
  - <path>:<line>
```

Then write a JSON array to `/tmp/thread-replies.json` describing the
replies to post on each thread:

```json
[
  {"rootCommentId": 123, "kind": "applied",
   "body": "Applied: <one-line summary>. @coderabbitai resolve"},
  {"rootCommentId": 456, "kind": "skipped",
   "body": "Skipping: <one-line reason>."},
  {"rootCommentId": 789, "kind": "already-fixed",
   "body": "Already addressed in <short-sha>. @coderabbitai resolve"}
]
```

The workflow uses this file to post the replies after the push.

## Step 5 — If nothing to apply

If every finding triages to `false-positive` or `already-fixed` —
i.e., you have no edits to make — do not write a commit message.
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
