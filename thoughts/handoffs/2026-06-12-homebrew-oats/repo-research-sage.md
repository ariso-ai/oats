---
date: 2026-06-12T16:20:00-04:00
type: repo-research
status: complete
repository: /Users/michaelgeiger/.codex/worktrees/2f14/sage
---

# Repository Research: Sage Desktop

## Overview

Sage is a Tauri v2 desktop app for Ariso/Oats, built with Vue 3, Vite, Rust,
and a Swift sidecar for local transcription and notes generation.

## Architecture & Structure

### Project Organization

- `src/` - Vue frontend views, composables, assets, and Tauri API wrapper.
- `src-tauri/` - Rust Tauri app, bundle configuration, icons, entitlements, and the Swift `ariso-stt` sidecar.
- `.github/workflows/desktop.yaml` - macOS validation, signed release, and R2 publish workflow.
- `.github/scripts/release-publish.sh` - Generates `latest.json` and publishes the DMG/updater artifacts to Cloudflare R2.
- `docs/superpowers/specs/` - Design specs for release/update behavior and desktop features.

### Technology Stack

- **Languages:** TypeScript/Vue, Rust, Swift.
- **Desktop Framework:** Tauri v2.
- **Build Tool:** npm scripts plus Cargo; release bundling through `tauri build`.
- **Testing:** Vitest for frontend/unit coverage; Cargo checks/tests for Rust.

### Key Files

- `README.md` - Canonical setup, local backend, release, signing, and distribution documentation.
- `src-tauri/tauri.conf.json` - Product name, version, bundle targets, macOS resources, and Tauri updater endpoint.
- `.github/scripts/release-publish.sh` - Stable R2 object keys: `desktop/latest.json`, `desktop/Ariso.app.tar.gz`, `desktop/Ariso.dmg`.
- `Casks/oats.rb` - Homebrew cask added for `brew tap ariso-ai/sage && brew install --cask oats`.

## Conventions & Patterns

### Code Style

- Shell release logic uses `set -euo pipefail`, explicit environment validation, and comments explaining release safety properties.
- README documents operational release steps close to the workflow they support.
- Desktop validation is expected to run host-side, not inside the OrbStack VM.

### Implementation Patterns

- Release artifacts are intentionally stable paths on R2, with `no-cache` headers and payloads uploaded before `latest.json`.
- The app bundle name is currently `Ariso.app`; the Homebrew token can be `oats` without renaming the signed bundle.

## Contribution Guidelines

### PR Requirements

- No PR template was found in this checkout.
- The desktop workflow validates `vite build` and `cargo build/check` on a self-hosted Apple Silicon runner.

### Coding Standards

- Run `npm run vite:build`, `npm test`, and Rust checks on the host.
- For casks, validate in a real tap layout because Homebrew rejects standalone cask paths for style/audit.

## Templates Found

| Template | Location | Purpose |
| --- | --- | --- |
| Apply fixes prompt | `.github/apply-fixes-prompt.md` | Automation prompt used by `.github/workflows/apply-fixes.yml`. |

## Key Insights

### What Makes This Project Unique

- The release pipeline already has a stable signed DMG URL, so Homebrew install support only needs a cask that points at the existing R2 artifact.
- The release workflow does not publish versioned DMG URLs; `version :latest` plus `sha256 :no_check` matches the current stable-object design.

### Gotchas / Important Notes

- The repo is private or not reachable to Homebrew audit via the GitHub URL, so the cask homepage should use the public `https://ariso.ai/` page.
- Because the cask download host differs from the homepage host, Homebrew requires a `verified:` parameter on the `url` stanza.

## Recommendations

### Before Contributing

1. Validate the cask through a temporary local tap path.
2. Keep release docs and the GitHub Release appended body in sync with the cask token.

### Patterns to Follow

- Follow `.github/scripts/release-publish.sh` for stable R2 release URLs.
- Follow Homebrew Cask cookbook syntax for `version`, `sha256`, `url`, `name`, `desc`, `homepage`, `depends_on`, and `app` stanzas.

## Sources

- `README.md`
- `src-tauri/tauri.conf.json`
- `.github/workflows/desktop.yaml`
- `.github/scripts/release-publish.sh`
- Homebrew Cask Cookbook: https://docs.brew.sh/Cask-Cookbook
- Adding Software to Homebrew: https://docs.brew.sh/Adding-Software-to-Homebrew
