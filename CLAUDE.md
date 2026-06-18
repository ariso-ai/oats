# oats — working with Claude Code

oats is a macOS menu-bar meeting recorder/notetaker: hit record, get a transcript and
notes. **Tauri v2 (Rust) + Vue 3 (TypeScript)**, Apple Silicon only. It runs either in
the cloud (Ariso) or fully offline on-device.

This repo is set up so Claude Code is productive on clone. When you trust the folder you
get the **superpowers** plugin (brainstorming, plans, TDD, systematic debugging) plus
this repo's own skills.

## Use the skills

Invoke these before working in the matching area (they encode this codebase's real
conventions, not generic framework docs):

- **`oats-architecture`** — orient first: cloud-vs-offline backend split, the `ariso-stt`
  speech pipeline, window topology, where specs live.
- **`oats-vue`** — any frontend change (views, composables, routing, Tailwind, TipTap).
- **`oats-tauri`** — any Rust/Tauri change, the invoke contract, capabilities, and the
  macOS build/test workarounds.
- **`oats-security`** — changes touching auth, tokens, capabilities, invoke commands,
  file paths, URL/deep-link opening, the sidecar, or the offline-mode privacy guarantee.
- **`oats-debugging`** — drive/inspect the running app via the `oats-desktop` MCP server.

## Recommended workflow

For features: `brainstorming → writing-plans → subagent-driven-development →
verification-before-completion`. For bugs: `systematic-debugging → test-driven-development
→ verification-before-completion`. Design specs live in `docs/superpowers/specs/`.

## Commands

```bash
npm run tauri:dev          # run the app
npm run tauri:dev:debug    # run with the MCP server (--features mcp)
npm run tauri:build        # bundle (exits non-zero on the updater signing step,
                           # but src-tauri/target/release/bundle/macos/oats.app is built)
npm test                   # frontend unit tests (Vitest)

# Rust tests need two macOS workarounds (see oats-tauri for why):
DYLD_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx" \
  cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
```

A fresh git worktree needs bootstrapping first (sidecar binaries, `npm ci`,
`npm run vite:build`) — see `oats-tauri`.

## Layout

- `src/` — Vue frontend. `main.ts` (bootstrap + router), `views/`, `composables/`,
  `tauri.ts` (typed `invoke` wrappers).
- `src-tauri/src/` — Rust backend. `commands.rs` (the frontend API), `main.rs` (setup +
  `invoke_handler`), domain modules, `capabilities/` (permission allowlist).
- `docs/superpowers/specs/` — design specs.

## `.claude/` directory

- `settings.json` — shared: registers the official marketplace, auto-enables superpowers.
- `settings.local.json` — personal overrides (not your concern when shared); it disables
  the `oats-desktop` MCP server by default — `oats-debugging` explains enabling it.
- `skills/` — the five oats skills above (auto-discovered; also packaged as the
  `oats-skills` plugin via `.claude-plugin/`).
- `commands/` — repo slash commands (e.g. `apply-coderabbit`).
