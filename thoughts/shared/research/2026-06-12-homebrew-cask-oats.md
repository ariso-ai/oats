---
date: 2026-06-12T16:20:00-04:00
type: external-research
topic: "Homebrew cask for Oats desktop app"
focus: best-practices
sources: [homebrew-docs]
status: complete
---

# Research: Homebrew Cask for Oats Desktop App

## Summary

Homebrew Cask is the appropriate Homebrew surface for a signed macOS `.app`
distributed in a DMG. The Oats release workflow already publishes a stable R2
DMG URL, so a tap-local cask can install the app without changing the existing
Tauri release pipeline.

## Key Findings

### Homebrew Cask Requirements

- A cask declares how software is downloaded and installed through stanzas such
  as `version`, `sha256`, `url`, `name`, `desc`, `homepage`, and `app`.
- Homebrew documents `sha256 :no_check` as the escape hatch when checksum
  verification is intentionally suppressed, commonly paired with `version :latest`
  for a moving stable URL.
- If the `url` host differs from the `homepage` host, Homebrew audit requires a
  `verified:` parameter on the `url` stanza.

### Recommended Implementation

Use a tap-local cask at `Casks/oats.rb`:

```ruby
cask "oats" do
  version :latest
  sha256 :no_check

  url "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/oats.dmg",
      verified: "pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/"
  name "Oats"
  desc "AI meeting recorder and notes app"
  homepage "https://ariso.ai/"

  depends_on macos: :sonoma

  app "oats.app"
end
```

## Pitfalls to Avoid

- Do not use the private GitHub repo URL as `homepage`; Homebrew online audit
  sees it as unreachable.
- Do not omit `verified:` for the R2 URL, because the download host differs from
  the public product homepage.
- Do not use a formula for the DMG-distributed macOS app; cask is the intended
  Homebrew package type for `.app` bundles.

## Sources

- Homebrew Cask Cookbook: https://docs.brew.sh/Cask-Cookbook
- Adding Software to Homebrew: https://docs.brew.sh/Adding-Software-to-Homebrew
