---
name: autopilot
description: Use only when the user explicitly invokes autopilot.
disable-model-invocation: true
---
Invoking this skill IS the user's approval of the design, spec, and plan.

1. Clarify: recon the codebase first, then ask ≤5 questions in ONE message,
   each with a recommended default. "Use defaults" = accept them all.
   Don't offer the visual companion.
2. Enrich: rewrite request + answers into a full implementation prompt
   (goal, non-goals, constraints, interfaces, acceptance criteria, test
   strategy). Save it as the spec in docs/superpowers/specs/. Run the spec
   self-review checklist; don't ask the user to review.
3. Plan: invoke superpowers:writing-plans on that spec. No approval pause.
4. Implement: invoke superpowers:subagent-driven-development. Create the
   worktree under .worktrees/ without asking. Resolve plan pre-flight
   conflicts yourself and list those decisions in the final summary.
5. Keep per-task review and the final branch review. Stop only for
   BLOCKED status, a failing test baseline, or the review circuit breaker.
