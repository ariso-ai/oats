# Vibe-Coder-Ready `.claude` Setup — Design

**Date:** 2026-06-18
**Status:** Approved for planning
**Topic:** Make the oats repo ready for "vibe coders" — a contributor who clones the
repo, opens Claude Code, and is productive immediately.

## Goal

When someone clones `oats` and opens Claude Code, the environment should bootstrap
itself: the superpowers plugin becomes available, and this repo's own Vue / Tauri /
architecture / debugging knowledge is loaded as skills — with no manual setup beyond
trusting the folder.

Delivery mechanism (decided): **marketplace + auto-enable**. We reference the
superpowers plugin from the official marketplace and ship our own domain skills as a
local plugin that the repo itself serves as a marketplace.

## Non-Goals

- Re-vendoring superpowers (we reference it, we don't copy it).
- Auto-enabling agent-skills or rust-analyzer-lsp (keep the shared config minimal —
  superpowers only). Contributors can opt in personally via `settings.local.json`.
- Changing application code. This is purely tooling/docs.

## Directory Layout (committed)

```
.claude/
  settings.json            NEW  shared: known marketplaces + enabled plugins
  settings.local.json      KEEP personal overrides (disables oats-desktop MCP, etc.)
  commands/apply-coderabbit.md  KEEP
.claude-plugin/
  marketplace.json         NEW  declares the "oats" marketplace (this repo)
  plugin.json              NEW  the "oats-skills" local plugin manifest
skills/                    NEW  our own domain skills
  oats-vue/SKILL.md
  oats-tauri/SKILL.md
  oats-architecture/SKILL.md
  oats-debugging/SKILL.md
CLAUDE.md                  NEW  root orientation: run/build/test + skill pointers
```

## Components

### 1. `.claude/settings.json` (shared, committed)

Registers the official marketplace and auto-enables superpowers. Minimal by decision.

```jsonc
{
  "extraKnownMarketplaces": {
    "claude-plugins-official": {
      "source": { "source": "github", "repo": "anthropics/claude-plugins-official" }
    }
  },
  "enabledPlugins": ["superpowers@claude-plugins-official"]
}
```

On first open the cloner trusts the folder, Claude Code registers the marketplace and
prompts to enable superpowers. Our own skills load from the repo-local plugin with no
network dependency. `settings.local.json` stays untouched and personal.

### 2. `.claude-plugin/marketplace.json` + `plugin.json` (repo-as-marketplace)

The repo doubles as a single-plugin marketplace named `oats`, exposing one plugin,
`oats-skills`, sourced from `./`. `plugin.json` points `skills` at `./skills`.

```jsonc
// .claude-plugin/marketplace.json
{
  "name": "oats",
  "owner": { "name": "ariso-ai" },
  "metadata": { "description": "oats domain skills for Claude Code" },
  "plugins": [
    { "name": "oats-skills", "source": "./",
      "description": "Vue, Tauri, architecture, and MCP-debugging skills for the oats codebase" }
  ]
}
```

```jsonc
// .claude-plugin/plugin.json
{
  "name": "oats-skills",
  "description": "Vue, Tauri, architecture, and MCP-debugging skills for the oats codebase",
  "author": { "name": "ariso-ai" },
  "homepage": "https://github.com/ariso-ai/oats",
  "license": "MIT",
  "skills": "./skills"
}
```

Note: project-local `skills/` are auto-discovered by Claude Code when the folder is
trusted, so the skills work even if a contributor never formally installs the plugin.
The marketplace/plugin manifests make the set installable/named and future-proof.

### 3. Skills (the real value — encode THIS codebase's conventions)

Each skill is one `SKILL.md` with YAML frontmatter (`name`, `description`) and a body
scoped to oats specifics, not generic framework docs.

- **`oats-vue`** — Vue 3 `<script setup>` + Composition API; Tailwind v4 via
  `@tailwindcss/vite`; `vue-router` across the multi-window UI; composables pattern
  (`src/composables`); TipTap editor (`@tiptap/vue-3`) usage; where views live
  (`src/views`). When to use: any frontend change.

- **`oats-tauri`** — Tauri v2 `invoke` ↔ Rust command layer (`src-tauri/src`);
  capabilities/permissions (`src-tauri/capabilities`); plugins in use (store, updater,
  notification, opener); multi-window topology (settings / library=Meetings / main);
  run/build commands; and the **test workaround** (`DYLD_LIBRARY_PATH` +
  `--test-threads=1`) plus fresh-worktree build prerequisites (copy
  `src-tauri/binaries`, `npm ci`, `vite:build`). When to use: any Rust/Tauri change or
  running the cargo suite.

- **`oats-architecture`** — the map: cloud (Ariso) vs offline backend abstraction;
  the `ariso-stt` Swift/MLX speech pipeline; high-level data flow; macOS TCC/permission
  testing gotcha (test on the bundle `ai.ariso.desktop`, not the adhoc dev binary);
  where specs live (`docs/superpowers/specs/`). When to use: orienting in the codebase
  or cross-cutting features.

- **`oats-debugging`** — drive and inspect the running app via the `oats-desktop` MCP
  server for debugging tasks: enabling the server (it is disabled in
  `settings.local.json` by default), launching the app, the window names
  (settings / library / main, main is headless), `show` before `execute_js`, and using
  `__TAURI_INTERNALS__.invoke`. Pairs with superpowers' `systematic-debugging`. When to
  use: reproducing/diagnosing runtime behavior in the desktop app.

### 4. `CLAUDE.md` (root) — the front door

Short orientation, not a manual:
- What oats is (one paragraph).
- Recommended workflow: `brainstorming → writing-plans → subagent-driven-development
  → verification-before-completion`.
- One-liners: `npm run tauri:dev`, `npm run tauri:build`, `npm test`, and the cargo
  test invocation with the `DYLD_LIBRARY_PATH` workaround.
- Directory map (`src/` frontend, `src-tauri/` backend, `docs/superpowers/specs/`).
- Explicit pointers: "use the `oats-vue` / `oats-tauri` / `oats-architecture` /
  `oats-debugging` skills."

CONTRIBUTING.md gets a short "Working with Claude Code" subsection linking to CLAUDE.md.

## Data Flow (bootstrap)

```
clone repo → open Claude Code → trust folder
  → settings.json registers claude-plugins-official marketplace
  → prompt to enable superpowers@claude-plugins-official
  → skills/ auto-discovered (oats-vue, oats-tauri, oats-architecture, oats-debugging)
  → CLAUDE.md loaded as project context
contributor is productive with zero extra setup
```

## Testing / Verification

- `settings.json`, `marketplace.json`, `plugin.json` are valid JSON.
- Each `SKILL.md` has valid frontmatter and is discoverable via the Skill tool.
- Sanity-check in this worktree that the skills appear and CLAUDE.md loads.
- Verify referenced commands/paths exist (e.g. `src-tauri/capabilities`,
  `src/composables`, the test workaround actually runs green).

## Risks / Open Questions

- Self-referencing marketplace source for the repo-local plugin: project-local
  `skills/` are auto-discovered regardless, so the plugin manifest is additive insurance
  rather than load-bearing. If the marketplace self-reference proves unsupported, the
  skills still work via auto-discovery.
- Skills must stay accurate as the codebase evolves; keep them terse and pointer-heavy
  rather than duplicating code that will drift.
