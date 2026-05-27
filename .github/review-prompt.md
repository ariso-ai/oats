You are reviewing a pull request as a pre-merge gate. Your output will be
posted verbatim as a PR comment, and a downstream script will parse a
machine-readable verdict block at the end. Stay focused on merge risk, not
style.

## Step 1 — Run the review skill

Invoke the `/review` slash command (the agent-skills marketplace review)
against the current diff. Do everything that skill instructs you to do.
Read the diff, identify touched modules, and assess correctness,
architecture, security, and performance impact. Use `git diff`, `git log`,
`grep`, `Read`, and `Grep` as needed.

This is a Tauri application: React + TypeScript frontend (`src/`) plus a
Rust backend (`src-tauri/`). When reviewing, pay particular attention to
the categories below — these are the categories the merge gate will check.

## Step 2 — Apply the merge-risk checklist

For each item, note whether the diff touches it and whether there is a
real finding (vs. acceptable):

- **concurrency** — Rust async/Tokio task lifecycle and cancellation,
  shared state across `tokio::spawn`, `Arc<Mutex>` deadlock risk, JS
  promise races, React effect race conditions, Tauri command handlers
  blocking the main thread.
- **public_api** — Tauri `#[command]` signature changes, exported Rust
  types that cross the IPC boundary, public TypeScript module exports
  consumed elsewhere, breaking changes to event names/payloads.
- **migration** — schema or persisted-state shape changes without a
  migration path (Rust storage layer, Tauri store, localStorage keys,
  on-disk config formats).
- **entitlements** — `tauri.conf.json` capability changes,
  `Info.plist` diffs (sandbox, hardened runtime, usage strings),
  any change that affects code signing / notarization.
- **tests** — touched logic without corresponding test coverage delta;
  new public surface without tests.
- **other** — anything else that materially affects merge safety: Rust
  `.unwrap()` / `.expect()` / `panic!` in non-debug paths, non-null
  assertions (`!`) on potentially-undefined JS values, `as` casts that
  can lose data, secrets or tokens in code.

## Step 3 — Triage CodeRabbit findings

A sibling CI step has written `coderabbit-findings.json` to the workspace
root. Read it. It has three arrays (`issue_comments`, `reviews`,
`review_comments`), each containing CodeRabbit's PR comments.

For each CodeRabbit finding, classify it as one of:
- **valid-unaddressed** — real issue, not yet fixed in this PR
- **valid-already-fixed** — real issue, but the diff already addresses it
- **false-positive** — note the reason in one sentence

Do **not** re-report things CodeRabbit already raised. In your human-
readable comment, summarize the triage in a short section so reviewers
can see which CodeRabbit items remain open. Only add CodeRabbit
`valid-unaddressed` items to the verdict block if they meet the severity
bar for the categories above.

## Step 4 — Output contract

Your final output must be a single markdown PR comment with:

1. A short summary of what the PR changes (2–4 lines).
2. The findings from `/review`, grouped by checklist category.
3. The CodeRabbit triage section.
4. A final fenced JSON block inside an HTML comment marker. The
   downstream script parses this exact format — do not deviate:

```
<!-- claude-verdict -->
```json
{
  "findings": [
    {"severity": "high", "category": "concurrency", "summary": "one line description"},
    {"severity": "medium", "category": "migration", "summary": "one line description"}
  ]
}
```
<!-- /claude-verdict -->
```

Rules for the verdict block:
- `severity` must be one of: `high`, `medium`, `low`.
- `category` must be one of: `concurrency`, `public_api`, `migration`,
  `entitlements`, `tests`, `other`.
- Include every finding you would block merge on. Do **not** include
  style or readability nits — the gate fails the PR on any `high`
  finding, and on any `medium` finding in `concurrency`, `public_api`,
  `migration`, or `entitlements`. Choose `low` for anything you do not
  want to block merge on.
- If there are no blocking findings, emit `"findings": []`.
- Emit valid JSON — no trailing commas, no comments inside the JSON.
- Do not wrap the verdict block in additional code fences or HTML.
