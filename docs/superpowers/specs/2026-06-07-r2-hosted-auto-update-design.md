# R2-Hosted Auto-Update Package — Design

**Date:** 2026-06-07
**Status:** Approved

## Goal

Host the desktop app's downloadable and auto-update artifacts on the existing
public Cloudflare R2 bucket instead of GitHub Releases. R2 becomes the **sole
host** for the binaries and updater manifest. The GitHub Release keeps only its
notes/tag and a link to the R2-hosted DMG; it no longer carries file
attachments.

## Background

- The Tauri v2 updater currently checks
  `https://github.com/ariso-ai/conflux/releases/latest/download/latest.json`
  (`src-tauri/tauri.conf.json` → `plugins.updater.endpoints`).
- The release job in `.github/workflows/release.yaml` signs + notarizes, builds
  `latest.json` (with `url` pointing at GitHub release-asset download URLs), and
  attaches the DMG + `.app.tar.gz` + `.sig` + `latest.json` to the GitHub
  Release via `softprops/action-gh-release`.
- The app **already** uses a public R2 bucket as a CDN for the notes LLM:
  `https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/...`
  (`src-tauri/src/model_manager.rs:204`). The bucket and public URL pattern
  already exist; this work reuses them under a new `/desktop/` prefix.
- Signature verification is unchanged: the updater verifies the tarball against
  the minisign `pubkey` embedded in `tauri.conf.json`. Only the *host* moves, so
  security guarantees are preserved regardless of where the file is served from.

## Decisions

| Question | Decision |
|---|---|
| Artifact scope | **Full replacement** — R2 is the sole host; GitHub Release carries no file attachments. |
| Public URL | **Existing r2.dev bucket** (`pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev`), under a `/desktop/` prefix. |
| Bucket layout | **Stable paths only** — overwrite the same object keys each release; no versioned archive on R2. |
| Upload tool | **AWS CLI** against R2's S3-compatible endpoint (R2 is S3-compatible; lighter than wrangler on the self-hosted runner). |
| GitHub Release | Keep notes/tag; **append a link** to the R2-hosted DMG. No file attachments. |

## Bucket Layout

Stable object keys, overwritten on every release:

```
desktop/latest.json        → updater manifest (endpoint target)
desktop/oats.app.tar.gz    → updater payload (referenced by latest.json `url`)
desktop/oats.dmg           → permanent human download link
```

Public URLs:

```
https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/latest.json
https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/oats.app.tar.gz
https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/oats.dmg
```

The `.app.tar.gz.sig` file is **not** uploaded. The Tauri updater reads the
signature from the `signature` field inside `latest.json`, not from a sidecar
`.sig` file, so it is unnecessary on R2.

## Changes

### 1. `src-tauri/tauri.conf.json` — updater endpoint

Replace the GitHub endpoint with the R2 manifest URL. `pubkey` unchanged.

```jsonc
"updater": {
  "endpoints": [
    "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/latest.json"
  ],
  "pubkey": "<unchanged>"
}
```

### 2. `src-tauri/src/model_manager.rs` — shared R2 base

Extract the R2 host into a single `const` so the desktop CDN and LLM CDN URLs
cannot drift, and build the existing `LLM_CDN_BASE` from it.

```rust
/// Public base for all app CDN assets (Cloudflare R2, r2.dev managed domain).
const R2_BASE: &str = "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev";

/// Public CDN base for the notes LLM files. The model is NOT ...
const LLM_CDN_BASE: &str =
    concat!(/* R2_BASE */ "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev",
            "/models/gemma-3-1b-it-qat-4bit");
```

> Note: Rust `const` cannot interpolate another `const &str` with `format!`.
> Either keep `LLM_CDN_BASE` as a full literal with a comment referencing
> `R2_BASE`, or use the `concat!` form above. The intent is a single documented
> source of truth for the host, not necessarily compile-time string composition.
> Implementation may choose whichever is cleanest; this is a light touch and not
> the core of the change.

### 3. `.github/workflows/release.yaml` — release job

**a. `latest.json` generation** — point the `url` at the stable R2 tarball path
instead of the GitHub asset URL. Keep version / notes / `pub_date` / `mandatory`
/ `signature` logic exactly as-is.

```bash
ASSET_URL="https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/oats.app.tar.gz"
```

**b. New step "Publish to R2"** — upload via AWS CLI to R2's S3 endpoint, to the
stable keys, with correct content types and cache headers:

```bash
aws s3 cp "$TARBALL" "s3://$R2_BUCKET/desktop/oats.app.tar.gz" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/gzip \
  --cache-control "no-cache, max-age=0, must-revalidate"

aws s3 cp "$DMG" "s3://$R2_BUCKET/desktop/oats.dmg" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/x-apple-diskimage \
  --cache-control "no-cache, max-age=0, must-revalidate"

# latest.json uploaded LAST, after the tarball it references is in place.
aws s3 cp latest.json "s3://$R2_BUCKET/desktop/latest.json" \
  --endpoint-url "$R2_ENDPOINT" \
  --content-type application/json \
  --cache-control "no-cache, max-age=0, must-revalidate"
```

Ordering matters: upload the tarball/DMG **before** `latest.json` so a client
that reads the new manifest never points at a not-yet-uploaded payload.

**c. Replace the asset-attachment step** — instead of uploading files,
`softprops/action-gh-release` appends the R2 DMG link to the release body:

```yaml
- name: Add R2 download link to GitHub Release
  uses: softprops/action-gh-release@v2
  with:
    tag_name: ${{ github.event.release.tag_name }}
    append_body: true
    body: |

      ---
      **Download:** https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/oats.dmg
```

No `files:` / `fail_on_unmatched_files:` — nothing is attached.

### 4. CI credentials — `release` GitHub Environment

Add an R2 S3 API token (created in the Cloudflare dashboard → R2 → Manage API
Tokens) as `release`-environment secrets, wired into the release job `env:`:

| Secret | Meaning |
|---|---|
| `R2_ACCESS_KEY_ID` | R2 token access key id (→ `AWS_ACCESS_KEY_ID`) |
| `R2_SECRET_ACCESS_KEY` | R2 token secret (→ `AWS_SECRET_ACCESS_KEY`) |
| `R2_ENDPOINT` | `https://<account-id>.r2.cloudflarestorage.com` |
| `R2_BUCKET` | bucket name backing the public `pub-...r2.dev` domain |

The AWS CLI reads `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` from env;
`AWS_DEFAULT_REGION=auto` is set for R2. The self-hosted macOS runner must have
the AWS CLI available (installed via `brew install awscli` if absent —
verified in the implementation plan).

### 5. README — CI / release docs

Update the "CI: Validation and Signed Releases" section: artifacts now publish to
R2 (`/desktop/...`) rather than the GitHub Release; the Release body links to the
R2 DMG. Document the four new `release`-environment secrets alongside the
existing Apple/Tauri ones.

## Error Handling & Edge Cases

- **Stale manifest (caching).** r2.dev managed domains edge-cache by default.
  All overwritten objects are uploaded with `no-cache, max-age=0,
  must-revalidate` so the updater and download link always resolve fresh
  content. This is the primary risk and its mitigation.
- **Partial publish.** Upload payloads (tarball, DMG) before `latest.json`. If
  an upload fails mid-step, `set -euo pipefail` aborts the job before
  `latest.json` is replaced, so clients keep seeing the previous consistent
  release.
- **Signature integrity.** Unchanged — the tarball is verified against the
  embedded minisign pubkey on the client. A corrupted/tampered R2 object fails
  verification and the update is rejected.
- **Missing AWS CLI on runner.** Implementation plan verifies/install the AWS
  CLI on the self-hosted runner as a prerequisite step.

## Testing / Verification

- **Manifest shape:** after a release, `curl .../desktop/latest.json` returns
  valid JSON with `version`, `url` (the R2 tarball), `signature`, `pub_date`,
  `mandatory`.
- **Cache headers:** `curl -I` on each object shows `cache-control: no-cache,
  max-age=0, must-revalidate` and the correct `content-type`.
- **End-to-end update:** install the previous version, publish a new release,
  confirm the app detects + applies the update from R2 (signature verifies).
- **Download link:** the GitHub Release body link resolves to the R2 DMG and
  installs.

## Out of Scope

- Versioned/archived artifact paths on R2 (explicitly deferred; stable paths
  only for now).
- Custom domain in front of the bucket (reuse the r2.dev managed domain).
- Non-macOS / multi-arch targets (current pipeline is `darwin-aarch64` only).
