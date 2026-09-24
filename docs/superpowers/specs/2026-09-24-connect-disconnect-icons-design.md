# Improve Connect and Disconnect button icons (issue #440)

## Problem

In `src/views/SettingsView.vue`, each remote-provider model row has an
icon-only action button that toggles between two states:

- **Connect** (`data-test="connect-key"`, `SettingsView.vue:299-314`) — a
  "link" glyph (two open link-halves, not yet joined).
- **Disconnect** (`data-test="remove-key"`, `SettingsView.vue:282-298`) — the
  same link glyph plus a diagonal strike, meant to read as "linked, click to
  break it."

A generic link/unlink icon doesn't read clearly as "connect this model
provider" — link icons are overloaded in UI (hyperlinks, chain associations,
copy-link actions) and don't specifically evoke plugging into a remote
service. The issue asks for icons whose meaning is immediately obvious for
this specific action.

## Goal

Replace the two SVGs with a plug / unplugged-plug pair:

- **Connect** — an electrical plug (two prongs above a rounded body, cord
  below): reads as "plug this provider in."
- **Disconnect** — the same plug shape with a diagonal strike through it,
  reusing this codebase's own existing convention (the current disconnect
  icon is already "connect icon + diagonal strike") for a state that's
  visually connected to its sibling but unambiguously "off."

Nothing else about the buttons changes: same `<button>` elements, same
`class="icon-btn"`, same `data-test`, `title`, `aria-label` bindings, same
`@click` handlers. Only the `<svg>` markup inside each button is replaced.

Reviewable as: open Settings → a cloud model provider row, see a plug icon on
the unconnected row's action button, click it, see the row's icon become a
struck-through plug, click again, see it revert — labels, tooltips, and
connection state behave exactly as before.

## Non-goals

- **The Calendar "Connect Calendar" button** (`SettingsView.vue:471-483`) —
  that's a labeled text button for a different feature (Google Calendar
  OAuth), not part of this icon-only row-action pair, and the issue's
  acceptance criteria describe the Connect/Disconnect row buttons only.
- **Changing hover/focus colors or button sizing** (`.icon-btn`,
  `.icon-btn--install` in the `<style>` block) — out of scope; the issue asks
  for an icon swap, not a restyle.
- **Introducing an icon library dependency** (e.g. `@heroicons/vue`) for
  these two icons — the surrounding row actions (install, delete) are
  hand-drawn inline SVGs in this codebase's existing house style (24x24
  viewBox, `stroke="currentColor"`, `stroke-width="2"`, round caps/joins,
  `fill="none"`); heroicons ship filled/differently-proportioned glyphs and
  don't have a plug icon. The new icons follow the same inline-SVG,
  stroke-based style as their siblings for visual consistency, per the
  issue's own acceptance criteria.
- **Touching `rowConnected`, `onConnectRow`, `onRemoveKey`, or any other
  connect/disconnect logic** — this is a pure visual change.

## Decisions

The maintainer (`shawnzhu`) did not comment before this run, so there are no
trusted-comment answers; all questions resolve to the recommended default.
The only untrusted comment (automated triage) asked whether the reporter had
a specific icon design in mind — no reporter reply exists, so it's treated as
open to implementer discretion, as that comment itself anticipated.

1. **Which concrete icon metaphor for "connect to a remote provider"?**
   Default (chosen): a plug / electrical-plug icon. **Why:** it's a
   widely-used, unambiguous "connect to a service" metaphor (distinct from
   the overloaded hyperlink icon it replaces) and is simple enough to draw
   cleanly at the 15x15px rendered size these buttons use
   (`.icon-btn svg { width: 15px; height: 15px }`).
2. **How does Disconnect visually relate to Connect?** Default (chosen):
   reuse this file's existing pattern — Disconnect is Connect's SVG plus one
   added diagonal `<line>` strike-through, exactly like the current
   link/unlink pair already does. **Why:** keeps the pair visually paired and
   consistent with the rest of the Oats UI's own established icon language,
   and is the least risky change (one shape, one modifier) to verify.
3. **Icon library vs. hand-drawn inline SVG?** Default (chosen): hand-drawn
   inline SVG matching the sibling install/delete icons' exact style
   attributes. **Why:** `@heroicons/vue`, this repo's documented icon set for
   *new* screens, has no plug glyph, and importing a component here would
   look inconsistent next to the other three hand-drawn icons in the same
   button group.
4. **Any change to button chrome (border/hover color, size)?** Default
   (chosen): none — only the `<svg>` children change. **Why:** the issue's
   acceptance criteria explicitly require behavior, labels, and a11y
   attributes to stay unchanged, and chrome changes weren't requested.
5. **Test coverage?** Default (chosen): no new tests needed beyond running
   the existing `SettingsView.test.ts` suite unmodified. **Why:** existing
   tests assert on `data-test`, `aria-label`, and click behavior, never on
   SVG path contents (confirmed by reading the test file), so they exercise
   the toggle behavior without caring which icon is drawn. Icon SVGs have no
   testable logic of their own.

## Design

Both icons are 24x24 viewBox, `fill="none" stroke="currentColor"
stroke-width="2" stroke-linecap="round" stroke-linejoin="round"` — identical
attributes to the existing install/delete icons in the same button group.

**Connect** (`SettingsView.vue:299-314`, replaces the current two-`<path>` +
`<line>` link glyph):

```html
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <path d="M9 2v6" />
  <path d="M15 2v6" />
  <path d="M7 8h10v4a5 5 0 0 1-10 0z" />
  <path d="M12 17v5" />
</svg>
```

Two prongs (`M9 2v6`, `M15 2v6`) above a rounded-bottom plug body (`M7
8h10v4a5 5 0 0 1-10 0z`), with a cord hanging below (`M12 17v5`).

**Disconnect** (`SettingsView.vue:282-298`, replaces the current struck-link
glyph): the same four paths, plus the diagonal strike already used by the
current disconnect icon:

```html
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <path d="M9 2v6" />
  <path d="M15 2v6" />
  <path d="M7 8h10v4a5 5 0 0 1-10 0z" />
  <path d="M12 17v5" />
  <line x1="4" y1="20" x2="20" y2="4" />
</svg>
```

The inline comments immediately above each button
(`SettingsView.vue:279-281`, `290-291`, `307-308`) describe the *old*
link-based icons and must be updated to describe the plug icons so future
readers aren't misled.

## Acceptance criteria

Carried directly from the issue, unchanged:

- [ ] The Connect button uses an icon whose meaning is immediately
      associated with establishing a connection to a remote provider.
- [ ] The Disconnect button uses an icon whose meaning is immediately
      associated with ending that connection.
- [ ] The icons are visually distinct and consistent with the rest of the
      Oats UI.
- [ ] Existing button labels, behavior, accessibility labels, and
      functionality remain unchanged.
- [ ] The updated icons render correctly in supported app states and
      platforms.

## Test strategy

- Run the existing `npm test` suite (`SettingsView.test.ts` in particular) —
  must pass unmodified, proving click behavior, `data-test` targeting,
  `title`/`aria-label` text, and connect/disconnect state toggling are all
  unaffected.
- Run `npm run vite:build` to catch any template/SVG syntax errors (unclosed
  tags, invalid attributes) that unit tests wouldn't surface.
- Manual/visual check is not possible in this non-interactive run (no
  running app, no display) — call this out explicitly as unverified in the
  PR body per the icons' actual rendered appearance at 15x15px.

## Risk

Low. This only touches two `<svg>` blocks (plus their preceding comments)
inside one `<template>` in `SettingsView.vue`; no script, prop, event, or
CSS changes. The only realistic failure mode is a malformed SVG path
breaking the Vue template compile, which `npm run vite:build` and `npm test`
(which mounts this component) will both catch.
