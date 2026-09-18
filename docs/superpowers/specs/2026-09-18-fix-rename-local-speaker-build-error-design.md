# Fix broken CI on main: `rename_local_speaker` fails to compile

## Goal

`cargo build --locked` fails on both the macOS and Windows CI jobs (Desktop
App workflow, `Validate (frontend + cargo build)` and
`Validate (windows-latest)`), blocking every PR on `main`. Restore a green
`cargo build --locked` on both platforms.

## Root cause

`storage::derive_notes_status` gained a third parameter, `notes_in_progress`
(added by the local-notes-checkpointing feature — see
`docs/superpowers/specs/2026-09-13-local-notes-checkpointing-design.md` — so
that an in-flight notes run outranks a preview note already on disk). Two of
its three call sites in `commands.rs` were updated
(`local_recording_status`, `delete_local_recording`), but the guard inside
`rename_local_speaker` (`src-tauri/src/commands.rs:2420`) was left calling the
old two-argument form:

```rust
crate::storage::derive_notes_status(has_note, meta.notes_error.as_deref());
```

This is a straight arity mismatch (`E0061`), so the crate fails to compile on
every platform — not a platform-specific issue despite the issue title
mentioning both macOS and Windows jobs; both run the same `src/commands.rs`.

Beyond the build break, the stale call site was also a latent correctness bug:
with only two arguments it could never have compiled with the old signature
once the third parameter was added, but conceptually — had it silently
defaulted has_note first — a checkpointed recording's preview note
(`has_note == true` while `notes_in_progress == true`) would have made
`rename_local_speaker` treat notes as `Ready` and allow a rename while the
final notes pass was still running, racing the same
`transcript.md`-overwrite hazard documented at `commands.rs:2377-2407`.

## Non-goals

- No changes to `derive_notes_status`'s signature or logic.
- No changes to dependency versions, `Cargo.lock`, or CI workflow files (both
  out of scope for a one-line call-site fix, and `Cargo.lock`/`.github/` are
  explicitly off-limits for this run).
- No broader audit of other call sites — `local_recording_status` and
  `delete_local_recording` already pass all three arguments correctly.

## Change

`src-tauri/src/commands.rs`, `rename_local_speaker`'s `Done`-status guard: pass
`meta.notes_in_progress` as the third argument, matching the sibling guard in
`delete_local_recording` immediately below it in the same file.

## Decisions

1. **Scope: fix only the compile error, or also close the latent
   preview-note race?** Default: fix both in the same one-line change, since
   passing the correct third argument does both — there is no additional
   surface area to cover separately. (default)
2. **Add a regression test?** Default: yes — add a unit test mirroring the
   existing `rename_local_speaker_rejects_while_ai_notes_are_pending`, but
   with a preview note present (`has_note == true`) and
   `notes_in_progress == true`, so a future regression that drops the third
   argument again fails a test instead of just failing to compile. (default)
3. **Investigate the two linked CI job URLs directly?** Default: no — this
   run has no network access. Root cause was found by reproducing
   `cargo build --locked` locally, which reproduces the exact `E0061` error
   independent of platform. (default)

No trusted comments were present on the issue (the `trusted` list is empty);
all decisions use the default.

## Acceptance criteria

- [x] `cargo build --locked` (from `src-tauri/`) succeeds on macOS.
- [x] `cargo build --locked --manifest-path src-tauri/Cargo.toml` succeeds
      (same command the Windows job runs, verified here on macOS since this
      run has no Windows runner; the fix is a platform-independent Rust
      arity error in shared source, so there is no reason it would behave
      differently on Windows).
- [x] `npm test` and `npm run vite:build` continue to pass (unaffected by
      this change, run to confirm no incidental regression).
- [x] A regression test covers the dropped `notes_in_progress` argument.

## Test strategy

- Reproduce: `cargo build --locked` in `src-tauri/` before the fix, confirm
  `E0061` at `commands.rs:2420`.
- Fix: pass `meta.notes_in_progress` as the third argument.
- Regression test:
  `rename_local_speaker_rejects_while_a_checkpointed_preview_note_is_in_flight`
  seeds a `Done` recording with a note file present (`has_note == true`) and
  `meta.notes_in_progress = true`, and asserts `rename_local_speaker` still
  rejects with the "AI notes are still generating" error.
- Full verification contract: `npm test`, `npm run vite:build`,
  `cargo build --locked --manifest-path src-tauri/Cargo.toml`, plus
  `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1` for
  the Rust suite (461 passed, 0 failed).
