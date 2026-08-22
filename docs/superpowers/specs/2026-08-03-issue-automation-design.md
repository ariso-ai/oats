# Issue automation design

Date: 2026-08-03

## Problem

Every issue filed against oats needs a human to read it, decide what kind of work
it is, and then either fix it or turn it into a spec. That first pass is
mechanical, and it is the step that decides how long everything downstream waits.

The repo already automates the *second half* of the development loop: CodeRabbit
reviews a PR, `apply-fixes.yml` runs Claude to apply the major findings, pushes a
commit, and CodeRabbit re-reviews. Once a PR exists, it largely drives itself.
Nothing automates the path from a filed issue *to* that PR.

Goal: reduce the human touches on an issue to one approval and one merge.

## Scope

Three event-driven workflows, modelled on `apply-fixes.yml`:

1. Classify every new issue as Bug / Feature / Task using GitHub issue types.
2. For bugs, produce a fix PR — but only after shawnzhu approves.
3. For feature requests, post an understanding of the request, and on shawnzhu's
   command turn the resulting discussion into a superpowers spec PR.

Out of scope: implementing a merged spec, triaging issues that already exist,
and any change to how PRs are reviewed or merged.

## Design principles inherited from `apply-fixes.yml`

These are not new inventions; they are the conventions the existing workflow
already proved, and every new workflow follows them.

- **Bash owns side effects.** Claude never pushes, comments, labels, or opens a
  PR. It writes a file to `/tmp`; a deterministic workflow step reads that file
  and performs the GitHub mutation. Claude's tool allowlist never includes
  `Bash(gh:*)` or `Bash(git push:*)`.
- **Prompts are versioned separately.** Each workflow reads its prompt from a
  `.github/*-prompt.md` file, so prompt edits are reviewable on their own.
- **Silence is a failure, not a pass.** If Claude produces neither a result nor
  an audit trail, the step exits non-zero rather than reporting a clean no-op.
- **Status is visible on the artifact.** A working label goes on before the
  expensive work and comes off in an `always()` step.
- **Untrusted text is never interpolated into shell.** Issue and comment bodies
  are extracted from `$GITHUB_EVENT_PATH` with `jq` and passed to Claude as a
  file path, never as a shell-expanded expression.

## Trust model

The repo is public, so anyone can file an issue and anyone can comment. Issue
text is therefore attacker-controlled and must be treated as data, never as
instructions. Two mechanisms enforce this.

**Capability, not persuasion.** The triage job — the one that reads the most
untrusted text — runs with an allowlist of `Read Grep Glob Write` and no `Bash`
at all. It cannot run a command, push a branch, or call the GitHub API no matter
what an issue body tells it to do. Its only outward effect is a JSON file the
workflow validates before acting on.

The tool allowlist is a usability control, though, not a sandbox, and two gaps
follow from that. First, `Read` is otherwise unrestricted on a *persistent*
runner, and triage publishes the model's output verbatim to a public issue — so
anything readable is potentially publishable. Second, `$HOME` survives between
runs (`actions/checkout` cleans the workspace, not the home directory), so a
file written there would still be present for the next agent invocation on that
machine. `.github/claude-agent-settings.json`, passed to every `claude -p` call
via `--settings`, denies reads of credentials and config and denies writes to
shell profiles and the agent's own settings. `Bash(grep:*)` is excluded from
every allowlist because it would read any file regardless of those rules.

**Enforcement lives in the workflow, not the prompt.** Prompt guardrails shape
good behavior; they do not constrain a subverted run. So the two properties that
actually matter are checked in bash after the agent finishes:

- *Diff denylist.* Before anything is committed, the fix workflow rejects
  changes to `.github/`, `.claude/`, `.agents/`, `.coderabbit.yaml`,
  `capabilities/`, signing config, and lockfiles. This is the important one:
  `apply-fixes.yml` checks out this PR's head and runs an agent inside it, so a
  committed change to agent settings or skills would be an agent-to-agent
  escalation needing no human merge. Shaping has the mirror-image rule — it may
  touch nothing *but* `docs/superpowers/specs/`.
- *Build gate.* The workflow re-runs `npm test`, `npm run vite:build`, and
  `cargo build --locked` itself rather than trusting the model's claim, so a PR
  cannot open on a red tree.

**Model-authored text is posted with the default `GITHUB_TOKEN`**, never with
the PAT below, and the PAT is scoped to `git push` and `gh pr create`. Otherwise
an agent comment would be authored as shawnzhu and could satisfy the `/shape`
gate — letting one workflow trigger the next with no human involved.

**A human gate before any write capability.** The job that *does* get edit tools
and push access only starts when shawnzhu adds the `autofix:approved` label:

```yaml
if: github.event.label.name == 'autofix:approved' &&
    github.event.sender.login == 'shawnzhu'
```

`sender.login` is the account that added the label, so the same label added by
anyone else — including another workflow — is ignored. The equivalent gate for
feature shaping is `comment.user.login == 'shawnzhu'`. This matches the
same-repo-head check `apply-fixes.yml` uses as its org-only gate.

Within the fix and shaping jobs, comment bodies are split by author: shawnzhu's
comments are labelled trusted requirements, everyone else's are labelled
untrusted context that may inform but never instruct.

## Workflow 1 — `issue-triage.yml`

**Trigger:** `issues: [opened]`, skipping bot authors and anything labelled
`skip-ai-triage`.

Claude reads the issue and the codebase (read-only) and writes
`/tmp/triage.json`:

```json
{"type": "Bug", "confidence": "high", "comment": "markdown or empty string"}
```

The workflow validates `type` against the three org issue types, then:

- Sets the type with `gh issue edit --type`.
- **Bug** — posts Claude's first-pass analysis: which subsystem it likely lives
  in, what repro detail is missing, and a note that shawnzhu can apply
  `autofix:approved` to start a fix.
- **Feature** — posts Claude's understanding of the request and its open
  questions, inviting discussion, and notes that shawnzhu can comment `/shape`
  to generate a spec.
- **Task** — sets the type only. An empty `comment` is a valid outcome here and
  the posting step is skipped.

A missing or invalid `triage.json` fails the run.

The issue templates already apply `bug` / `enhancement` labels. Those are a hint
to the classifier, not authoritative — a "feature request" that describes broken
behaviour is a Bug. Triage sets the *type* and leaves template labels alone.

## Workflow 2 — `issue-autofix.yml`

**Trigger:** `issues: [labeled]` behind the shawnzhu gate above.

Branches `autofix/issue-<N>` from `main`, gathers the issue and its comments
(author-partitioned), and runs Claude with the same tool allowlist and the same
guardrails as `apply-fixes.yml`: no workflow files, no capability or signing
config, no removing error handling, no public API signature changes without
checking callers.

Build verification is the real contract this workflow enforces:

```bash
npm test
npm run vite:build
( cd src-tauri && cargo build --locked )
```

`cargo build` requires the `ariso-stt` Swift sidecar to exist under the
gitignored `src-tauri/binaries/`, because `tauri.conf.json` declares it as an
`externalBin`. `actions/checkout` runs `git clean -ffdx`, which deletes it on
every run. This workflow therefore reuses `desktop.yaml`'s sidecar cache and
`xcodebuild` steps, keyed on the Xcode toolchain fingerprint plus the sidecar
sources, so a warm cache makes it a no-op and a cold one still produces a
working tree.

Two outcomes, both of which must leave a record:

- **Fix produced** — Claude writes `/tmp/commit-message.txt`, `/tmp/pr-title.txt`
  (Conventional Commits, since release-please reads it), and `/tmp/pr-body.md`.
  The workflow commits as `github-actions[bot]`, pushes, and opens a **non-draft**
  PR with `Fixes #N`. Non-draft is deliberate: it hands the branch straight to
  the existing CodeRabbit → `apply-fixes.yml` loop, which is what closes the gap
  between a first attempt and a mergeable change. Merging stays human.
- **No confident fix** — Claude writes `/tmp/no-fix-report.md` explaining what it
  investigated and what it needs. The workflow posts that on the issue and
  removes `autofix:approved`, so re-applying the label after adding detail
  retriggers cleanly.

Producing neither is a hard failure.

## Workflow 3 — `feature-shaping.yml`

**Trigger:** `issue_comment: [created]` where the commenter is shawnzhu, the
body starts with `/shape`, and the thread is an issue rather than a PR. A
follow-up step confirms the issue type is `Feature` and exits early if not.

Claude reads the issue and the whole discussion — shawnzhu's comments as
requirements, everyone else's as context — and writes a design spec to
`docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`, matching the 27 specs
already in that directory. The workflow supplies the date so the filename cannot
drift from the run.

It then commits to `spec/issue-<N>`, opens a PR titled `docs: …` (no release
bump), and comments the PR link back on the issue. Review and merge are human;
implementing a merged spec is a natural later extension and is deliberately not
built now.

## Cross-cutting

**Concurrency** is grouped per issue
(`${{ github.workflow }}-${{ github.event.issue.number }}`), so different issues
process in parallel while one issue never runs the same workflow twice at once.

**Runner** is `[self-hosted, macOS, ARM64]` for all three, reusing the existing
`claude` authentication. The fix workflow needs macOS for the Tauri build
regardless; triage stays on the same runner for consistency and is constrained
by its tool allowlist rather than by runner isolation.

**Token.** GitHub deliberately does not trigger workflows from events caused by
the default `GITHUB_TOKEN`, so a PR opened with it would never run
`desktop.yaml` CI — which would quietly remove the strongest check on an
agent-authored change. Both PR-opening workflows therefore use
`${{ secrets.AUTOFIX_TOKEN || github.token }}` for the checkout token and `gh`.
The secret is optional: without it everything still runs and CodeRabbit (a
GitHub App, unaffected by the rule) still reviews, but Actions CI is skipped on
those PRs. Setting `AUTOFIX_TOKEN` to a PAT or GitHub App token with repo scope
is the one piece of manual setup this design needs.

**New repo state:** three labels — `autofix:approved`, `skip-ai-triage`, and
`ai-failed` — created idempotently by `issue-triage.yml`, which is the one
workflow that runs for every issue. Creating them matters: a label that has
never existed does not appear in the sidebar picker, so shawnzhu could not apply
the approval label at all.

**Failure is always visible.** Each workflow has an `if: failure()` step that
comments on the issue with a link to the run log. Without it a crashed run is
indistinguishable from one that never triggered — the worst case being `/shape`
leaving nothing but an 👀 reaction.

## Residual risk

Worth stating plainly rather than implying the design is airtight.

The fix workflow gives an agent edit tools and build commands on a persistent
self-hosted runner, and `npm test` executes whatever `package.json` says. An
agent that has been successfully injected can therefore run code on that runner
*during* its own job. What it cannot do is persist anything: the diff denylist
blocks changes to agent inputs, the build gate blocks a red PR, no token is in
its environment, and the workspace is cleaned on the next checkout. The bound on
that exposure is shawnzhu's approval — this workflow only ever starts because a
human read the issue and decided it was worth an attempt.

Triage has no such gate, being open to anyone on the internet, which is why it
has no shell at all. Its remaining exposure is cost and runner contention: N
filed issues queue N LLM jobs on the same Mac that runs release CI. If that
becomes a problem, the fix is a distinct runner label so triage cannot starve
releases, plus a cheap pre-filter before the model is invoked.

## Resulting loop

A bug goes: filed → typed and analysed automatically → shawnzhu applies one
label → fix PR opens → CodeRabbit and `apply-fixes.yml` iterate on it → shawnzhu
merges.

A feature goes: filed → typed, with the agent's understanding posted for
discussion → shawnzhu comments `/shape` → spec PR opens → shawnzhu merges.

Two human actions per item, both of them decisions rather than typing.
