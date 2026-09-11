# Autonomous mode — running oats development from the issue tracker

This is a guide for whoever is steering the product. It assumes you live in the
issue tracker, not in the codebase, and that your job is deciding *what* gets
built and whether the result is right — not writing the diff.

Four GitHub Actions workflows watch this repo's issues. They classify every new
issue, and on your say-so they attempt a bug fix, implement a task end to end,
or turn a feature discussion into a design spec. Each produces a pull request.
None merges anything.

Your whole surface area is **one label and one comment**. Everything else is
either automatic or still a human's job.

## The loop

```
                        you file / someone files an issue
                                      │
                                      ▼
                          ┌───────────────────────┐
                          │  automatic triage     │  ~1 min
                          │  sets type + comments │
                          └───────────────────────┘
                                      │
              ┌───────────────────────┼───────────────────────┐
              ▼                       ▼                       ▼
            Bug                    Feature                  Task
              │                       │                       │
   you add `autofix:approved`   discussion happens   you add `autofix:approved`
              │                       │                       │
              │              you comment `/shape`             │
              ▼                       ▼                       ▼
        fix PR opens            spec PR opens     autopilot: spec → plan →
              │                       │           implement → PR opens
              │                       │                       │
   CodeRabbit reviews it,             │          CodeRabbit reviews it,
   apply-fixes.yml applies            │          apply-fixes.yml applies
   the findings, CI runs              │          the findings, CI runs
              │                       │                       │
              ▼                       ▼                       ▼
         ═══════════════════ you review and merge ═══════════════════
```

Two decisions per item. Both are judgment calls — "is this worth an attempt?"
and "is this result good?" — which is the part that should stay with you.

## Your two levers

| Lever | What it does | Works on |
|---|---|---|
| `autofix:approved` label | Starts a fix attempt, opens a PR | Issues typed **Bug** |
| `autofix:approved` label | Runs autopilot — spec, plan, implementation — opens a PR | Issues typed **Task** |
| `/shape` comment | Writes a design spec, opens a PR | Issues typed **Feature** |

The label routes on the issue's type: the same `autofix:approved` starts the
bug-fix workflow on a Bug and autopilot on a Task.

**Only `shawnzhu` can pull any lever.** The workflows check who applied the
label and who wrote the comment. The same label added by anyone else — or by
another bot — does nothing at all. This is deliberate: the repo is public, so
anyone can file an issue or comment, and an agent with commit access should not
take instructions from the internet.

That has a consequence worth internalizing: **when a teammate comments on an
issue, the agent reads it as context, not as a requirement.** Only your comments
are treated as decisions. If someone else lands on the right answer in the
thread, restate it in your own comment before you pull the lever, or it may not
carry the weight you expect.

## Path 1 — A bug comes in

**What happens without you.** Within about a minute of the issue being filed,
it gets typed `Bug` and picks up a comment containing a first-pass analysis:
which subsystem probably owns the behavior with real file paths, what
reproduction detail is missing, and anything suggesting it isn't actually a bug
(misconfiguration, expected behavior, already fixed in a newer version).

Read that comment before doing anything else. It usually tells you whether the
report is actionable, and it names the specific fields to ask the reporter for
when it isn't.

**Your decision.** If the issue looks real and well-specified, add
`autofix:approved`. If it's missing repro steps, ask the reporter first — a fix
attempt on a vague report burns ~15 minutes and usually comes back asking the
same questions.

**What you get back**, one of three things:

- **A pull request.** Non-draft on purpose, so it flows straight into the
  existing review loop: CodeRabbit reviews it, `apply-fixes.yml` applies the
  findings, CI runs. The PR body states the root cause and — importantly — what
  it could *not* verify. It ran unit tests and both builds; it did not run the
  app, use a real cloud account, or touch a device permission. Treat that
  section as your test plan.
- **A "no confident fix" comment.** It explains what it investigated, why it
  stopped, and what would unblock it. This is a legitimate outcome, not a
  failure. Answer the questions and re-apply the label.
- **A failure notice** with a link to the run log, if the machinery itself
  broke.

**Re-running.** The label clears itself after every run, so re-applying it is
always a clean retry. If a PR is already open for that issue, the workflow
declines to start a second attempt and says so — iterate on the open PR instead,
or close it and re-apply for a fresh run from scratch.

## Path 2 — A feature request comes in

This is the path where your input matters most, so it's worth understanding the
shape.

**What happens without you.** The issue gets typed `Feature` and picks up a
comment that restates the underlying problem in the agent's own words, names
which parts of oats it would touch, flags whether it interacts with the
cloud/offline split, and asks **two or three specific open questions**.

Those questions are the point. They're chosen to be the ones whose answers
change the design.

**The discussion.** Anyone can weigh in, and the thread is genuinely useful
input — but remember only your comments count as decisions. Let the thread run
until the open questions have answers.

**Your decision — and the highest-leverage thing you do.** Comment `/shape`.
The rest of that comment is a direct instruction to the spec writer, and it
overrides both the original request and the discussion.

A bare `/shape` produces a spec built on the agent's own reading of the thread.
A `/shape` carrying your decisions produces the spec you actually wanted. For
example:

> `/shape`
>
> Save-to-disk via the native save dialog, not the share sheet. Include the
> transcript, but behind a checkbox that defaults to off. Keep it to local
> recordings for a first cut — cloud can follow once the shape is proven.

Three sentences, and they settle the three open questions, cut the scope, and
put the deferred half in Non-goals. Spend your effort here rather than reviewing
a spec built on guesses.

**What you get back.** A pull request adding one file to
`docs/superpowers/specs/`, matching the specs already there: Problem, Goal,
Non-goals, Design, Cloud vs offline, Error handling, Testing, Open questions.
Nothing else — the workflow refuses to commit anything outside that directory.

Read **Non-goals** and **Open questions** first. Non-goals is where your scope
cuts either landed or didn't. Open questions is where it's telling you it
guessed.

**Merging a spec lands the design only.** The issue deliberately stays open to
track implementation, which is still a human's job. Implementing a merged spec
is not automated.

**If the request is too vague to design**, you get a comment listing what it
needs instead of a spec. Answer and comment `/shape` again.

## Path 3 — A task comes in

**What happens without you.** Chores, refactors, docs, and anything too vague to
be a bug or a feature get typed `Task`, usually with no comment. They sit in the
tracker as normal work until you decide otherwise.

**Your decision.** Add `autofix:approved` and the task goes to **autopilot**:
the same superpowers pipeline you'd run by hand, end to end, with no approval
pauses. Applying the label *is* your approval of the design, spec, and plan it
produces — so this is the lever where the issue text matters most. Write the
task, or restate it in your own comment, as the implementation request you want
built.

**What runs.** In order, in a worktree under `.worktrees/` on
`autopilot/issue-<N>`:

1. **Clarify** — it reads the codebase and picks up to five questions whose
   answers change the implementation, each with a recommended default. Nobody is
   there to answer, so it takes the defaults — except where *your* comments on
   the issue already answer a question, which wins. To steer it, answer the
   likely questions in a comment before you apply the label.
2. **Enrich** — rewrites the request and answers into a full spec (goal,
   non-goals, constraints, interfaces, acceptance criteria, test strategy),
   committed to `docs/superpowers/specs/`.
3. **Plan** — an implementation plan from that spec (local to the run; plans are
   not committed).
4. **Implement** — subagent-driven development: a fresh implementer per plan
   task, a review after each, and a final whole-branch review. Plan conflicts
   it resolves itself, and it lists every such ruling for you.

**What you get back**, one of three things:

- **A pull request.** Non-draft, so CodeRabbit and `apply-fixes.yml` pick it up
  like any other. The body leads with **Decisions** (every question and whether
  it took the default or your answer) and **Rulings I made** (every conflict it
  resolved on your behalf, with what it costs if wrong). Read those two first —
  they are where it guessed. Merging closes the task.
- **A "stopped" comment.** Autopilot stops only for a blocked task it can't
  resolve, a failing test baseline, or the review circuit breaker (a task still
  failing review after five fix rounds). The comment says where and why. Partial
  work is pushed to `autopilot/issue-<N>` for you to inspect — no PR.
- **A failure notice** with a link to the run log, if the machinery broke.

**Re-running** works like the bug path: the label clears after every run, a
re-apply starts over from `main` (overwriting a stopped run's branch), and an
open PR for the task blocks a second attempt.

**Expect it to be slow.** Every plan step gets its own implementer and reviewer
subagents, so a run can take hours; the job times out at five.

If something lands as `Task` that you think is really a bug or a feature,
change the issue type by hand — the type is what routes it, so a `Bug` sends
`autofix:approved` to the bug-fix workflow and a `Feature` accepts `/shape`.

## Writing issues the agent can act on

Triage reads whatever it's given and investigates the real codebase, so the
quality of the input drives the quality of everything downstream.

**For bugs**, the things that change whether a fix attempt succeeds:

- **Reproduction steps that actually trigger it.** The single biggest factor.
- **Which backend** — Ariso cloud or local/on-device. Many code paths diverge
  completely, and without this the agent has to guess which one you mean.
- **Version and OS.** Distinguishes a live bug from one already fixed.
- **The literal error text or UI string.** This is what gets grepped. A quoted
  message is worth more than a paragraph describing it.
- **What you observed versus what you expected**, kept separate.

**For features**, describe the problem before the solution. The triage comment
and the eventual spec both open with the underlying need, and a request phrased
purely as an implementation ("add a button that…") gives the agent nothing to
reason about when the obvious implementation turns out to be wrong. Say who is
blocked, on what, and what they do today instead.

The issue templates already ask for most of this. They're a floor, not a
ceiling — the free-text fields are what get read most closely.

## Labels

| Label | Meaning |
|---|---|
| `autofix:approved` | **You set this.** Starts a fix attempt on a Bug, autopilot on a Task. Cleared automatically after every run, so re-applying is always a clean retry. |
| `autofix:working` | Set by the workflow while a run is in flight. Cleared on every outcome, including a crash. Purely informational. |
| `skip-ai-triage` | Set it *before* filing, or on an issue you don't want touched, to opt out of automatic triage entirely. |
| `ai-failed` | The machinery broke. Comes with a comment linking the run log. Remove it once handled. |

The `bug` and `enhancement` labels the issue templates apply are separate from
issue *types*. Types are what drive routing; the agent treats the template label
as a hint and will overrule it — a report filed on the feature template that
describes broken behavior gets typed `Bug`.

## When it goes wrong

Every workflow posts a comment with a link to its run log when it fails, so a
crashed run is never silent. The common cases:

- **Nothing happened at all.** Check the issue isn't labelled `skip-ai-triage`,
  and that a bot didn't file it (bot-authored issues are skipped so automation
  can't trigger automation).
- **`/shape` did nothing but add an 👀 reaction.** Almost always because the
  issue isn't typed `Feature`. You'll get a comment saying so.
- **The label did nothing.** Confirm you applied it and not someone else, and
  that the issue is open.
- **The label started the wrong workflow.** The type decides: check it was
  `Task` (autopilot) or `Bug` (fix attempt) *before* you applied the label.
  Changing the type afterwards doesn't redirect a run that already started.

## What it will never do

Worth knowing so you don't wait for something that isn't coming:

- **Merge anything.** Every PR waits for a human.
- **Close an issue.** A merged PR closes its issue via `Fixes #N` or
  `Closes #N`; nothing else does.
- **Implement a merged spec from `/shape`.** Shaping stops at the design. To
  have autopilot build it, file a Task that points at the spec.
- **Change its own configuration.** The workflows that write code refuse to
  push changes to `.github/`, `.claude/`, `.agents/`, the Tauri capability
  allowlist, signing config, or dependency files. Work that genuinely needs one
  of those comes back as "no confident fix" or a stop, with the reason.
- **Act on anyone else's instructions.** Including instructions embedded in an
  issue body. That's what gating every lever on you is for.

## Related

- `docs/superpowers/specs/2026-08-03-issue-automation-design.md` — the design
  behind these workflows, including the trust model and its residual risks.
- `.github/workflows/issue-triage.yml`, `issue-autofix.yml`,
  `issue-autopilot.yml`, `feature-shaping.yml` — the implementations.
  `issue-autopilot.yml`'s header states its own, wider trust model: its agent
  has a full shell, so its defenses are structural.
- `.agents/skills/autopilot/SKILL.md` — the autopilot skill. You can also run
  it locally with `/autopilot <request>`, where it asks its questions for real.
- `.github/*-prompt.md` — what each agent is actually told to do. Worth reading
  if the output is consistently off in some way; these are the knobs.
